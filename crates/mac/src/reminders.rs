//! Reminders via EventKit (the same store the iOS/macOS Reminders app uses).
//!
//! Timed reminders get an absolute alarm, like ones made in the Reminders app, so they
//! also pop up on the iPhone.

use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use bb_proto::{ApiError, ErrorCode, Event, EventSink, NewReminder, Reminder, ReminderList, Response, Timestamp};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAlarm, EKAuthorizationStatus, EKCalendar, EKEntityType, EKEventStore, EKReminder};
use objc2_foundation::{NSArray, NSCalendar, NSCalendarUnit, NSDate, NSDateComponentUndefined, NSDateComponents, NSError, NSString};

struct Store(Retained<EKEventStore>);
unsafe impl Send for Store {}

pub struct Reminders {
    store: Mutex<Option<Store>>,
}

fn millis(date: &NSDate) -> Timestamp {
    (date.timeIntervalSince1970() * 1000.0) as Timestamp
}

fn date(ms: Timestamp) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(ms as f64 / 1000.0)
}

fn ns_err(context: &str, err: &NSError) -> ApiError {
    ApiError::internal(format!("{context}: {}", err.localizedDescription()))
}

/// Due date components → (ms, has_time). Date-only reminders resolve to local midnight.
fn due_of(components: &NSDateComponents) -> Option<(Timestamp, bool)> {
    let has_time = components.hour() != NSDateComponentUndefined;
    NSCalendar::currentCalendar().dateFromComponents(components).map(|d| (millis(&d), has_time))
}

fn components(ms: Timestamp, has_time: bool) -> Retained<NSDateComponents> {
    let mut units = NSCalendarUnit::Year | NSCalendarUnit::Month | NSCalendarUnit::Day;
    if has_time {
        units |= NSCalendarUnit::Hour | NSCalendarUnit::Minute;
    }
    NSCalendar::currentCalendar().components_fromDate(units, &date(ms))
}

fn to_proto(r: &EKReminder) -> Reminder {
    unsafe {
        let due = r.dueDateComponents().and_then(|c| due_of(&c));
        Reminder {
            id: r.calendarItemIdentifier().to_string(),
            list_id: r.calendar().map(|c| c.calendarIdentifier().to_string()).unwrap_or_default(),
            title: r.title().to_string(),
            notes: r.notes().map(|s| s.to_string()).filter(|s| !s.is_empty()),
            due: due.map(|d| d.0),
            due_has_time: due.is_some_and(|d| d.1),
            completed: r.isCompleted(),
            completed_at: r.completionDate().map(|d| millis(&d)),
            priority: r.priority().min(9) as u8,
        }
    }
}

impl Reminders {
    pub fn new() -> Self {
        Self { store: Mutex::new(None) }
    }

    fn with_store<T>(&self, f: impl FnOnce(&EKEventStore) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let mut guard = crate::lock_within(&self.store, Duration::from_secs(15)).ok_or_else(|| {
            ApiError::internal("Reminders on the Mac isn't responding (an EventKit call is stuck). Restart bbd on the Mac.")
        })?;
        if guard.is_none() {
            let store = unsafe { EKEventStore::new() };
            let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Reminder) };
            if status != EKAuthorizationStatus::FullAccess {
                let (tx, rx) = mpsc::channel();
                let block = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
                    let _ = tx.send(granted.as_bool());
                });
                unsafe { store.requestFullAccessToRemindersWithCompletion(RcBlock::as_ptr(&block)) };
                if !rx.recv_timeout(Duration::from_secs(120)).unwrap_or(false) {
                    return Err(ApiError::new(
                        ErrorCode::PermissionDenied,
                        "Reminders access denied. Allow bbd in System Settings → Privacy & Security → Reminders.",
                    ));
                }
            }
            *guard = Some(Store(store));
        }
        f(&guard.as_ref().unwrap().0)
    }

    pub fn lists(&self) -> Result<Response, ApiError> {
        self.with_store(|store| {
            let list = unsafe { store.calendarsForEntityType(EKEntityType::Reminder) };
            Ok(Response::ReminderLists(
                list.iter()
                    .map(|c| {
                        let id = unsafe { c.calendarIdentifier() }.to_string();
                        ReminderList {
                            color: crate::calendar::color_for(&id),
                            title: unsafe { c.title() }.to_string(),
                            writable: unsafe { c.allowsContentModifications() },
                            id,
                        }
                    })
                    .collect(),
            ))
        })
    }

    fn list_calendar(store: &EKEventStore, id: &str) -> Result<Retained<EKCalendar>, ApiError> {
        unsafe { store.calendarWithIdentifier(&NSString::from_str(id)) }.ok_or_else(|| ApiError::not_found("reminder list"))
    }

    fn find(store: &EKEventStore, id: &str) -> Result<Retained<EKReminder>, ApiError> {
        unsafe { store.calendarItemWithIdentifier(&NSString::from_str(id)) }
            .and_then(|item| item.downcast::<EKReminder>().ok())
            .ok_or_else(|| ApiError::not_found("reminder"))
    }

    pub fn reminders(&self, list_id: Option<&str>, include_completed: bool) -> Result<Response, ApiError> {
        let rx = self.with_store(|store| {
            let calendars = list_id.map(|id| Self::list_calendar(store, id)).transpose()?.map(|c| NSArray::from_retained_slice(&[c]));
            let predicate = unsafe {
                if include_completed {
                    store.predicateForRemindersInCalendars(calendars.as_deref())
                } else {
                    store.predicateForIncompleteRemindersWithDueDateStarting_ending_calendars(None, None, calendars.as_deref())
                }
            };
            // The fetch is asynchronous; convert on EventKit's queue and hand back plain data.
            let (tx, rx) = mpsc::channel::<Vec<Reminder>>();
            let block = RcBlock::new(move |found: *mut NSArray<EKReminder>| {
                let list = unsafe { found.as_ref() }.map(|a| a.iter().map(|r| to_proto(&r)).collect()).unwrap_or_default();
                let _ = tx.send(list);
            });
            unsafe { store.fetchRemindersMatchingPredicate_completion(&predicate, &block) };
            Ok(rx)
        })?;
        let list = rx.recv_timeout(Duration::from_secs(30)).map_err(|_| ApiError::internal("timed out fetching reminders"))?;
        Ok(Response::Reminders(list))
    }

    pub fn create(&self, new: &NewReminder) -> Result<Response, ApiError> {
        self.with_store(|store| unsafe {
            let calendar = match &new.list_id {
                Some(id) => Self::list_calendar(store, id)?,
                None => store.defaultCalendarForNewReminders().ok_or_else(|| ApiError::not_found("default reminder list"))?,
            };
            let reminder = EKReminder::reminderWithEventStore(store);
            reminder.setCalendar(Some(&calendar));
            reminder.setTitle(Some(&NSString::from_str(&new.title)));
            if let Some(notes) = &new.notes {
                reminder.setNotes(Some(&NSString::from_str(notes)));
            }
            reminder.setPriority(new.priority.min(9).into());
            if let Some(due) = new.due {
                reminder.setDueDateComponents(Some(&components(due, new.due_has_time)));
                if new.due_has_time {
                    reminder.addAlarm(&EKAlarm::alarmWithAbsoluteDate(&date(due)));
                }
            }
            store.saveReminder_commit_error(&reminder, true).map_err(|e| ns_err("saving reminder", &e))?;
            Ok(Response::Reminder(to_proto(&reminder)))
        })
    }

    pub fn set_completed(&self, id: &str, completed: bool) -> Result<Response, ApiError> {
        self.with_store(|store| unsafe {
            let reminder = Self::find(store, id)?;
            reminder.setCompleted(completed);
            store.saveReminder_commit_error(&reminder, true).map_err(|e| ns_err("saving reminder", &e))?;
            Ok(Response::Ok)
        })
    }

    pub fn delete(&self, id: &str) -> Result<Response, ApiError> {
        self.with_store(|store| unsafe {
            let reminder = Self::find(store, id)?;
            store.removeReminder_commit_error(&reminder, true).map_err(|e| ns_err("deleting reminder", &e))?;
            Ok(Response::Ok)
        })
    }

    /// Same fingerprint poll as Calendar: EventKit's change notification needs a run loop.
    pub fn watch(&self, sink: EventSink) {
        // One long-lived store, never released (see `crate::run_main_loop`): with the main
        // run loop running it follows changes from the iPhone itself.
        let reminders = Reminders::new();
        std::thread::spawn(move || {
            let mut last: Option<u64> = None;
            loop {
                std::thread::sleep(Duration::from_secs(10));
                let Ok(Response::Reminders(list)) = reminders.reminders(None, false) else { continue };
                let fingerprint = list.iter().fold(list.len() as u64, |h, r| {
                    r.id.bytes()
                        .chain(r.title.bytes())
                        .fold(h ^ r.due.unwrap_or(0) as u64 ^ u64::from(r.completed), |h, b| h.wrapping_mul(1_099_511_628_211) ^ u64::from(b))
                });
                if last.is_some_and(|l| l != fingerprint) {
                    sink(Event::RemindersChanged);
                }
                last = Some(fingerprint);
            }
        });
    }
}
