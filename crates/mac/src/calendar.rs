//! Calendar via EventKit.

use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use bb_proto::{ApiError, Calendar as Cal, CalendarEvent, ErrorCode, Event, EventSink, NewEvent, Response, Timestamp};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAuthorizationStatus, EKCalendar, EKEntityType, EKEvent, EKEventStore, EKSpan};
use objc2_foundation::{NSDate, NSError, NSString};

/// EventKit objects are usable from any thread as long as access is serialized.
struct Store(Retained<EKEventStore>);
unsafe impl Send for Store {}

pub struct Calendar {
    store: Mutex<Option<Store>>,
}

fn date(ms: Timestamp) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(ms as f64 / 1000.0)
}

fn millis(date: &NSDate) -> Timestamp {
    (date.timeIntervalSince1970() * 1000.0) as Timestamp
}

/// A stable pleasant color per calendar (EventKit's color needs AppKit color spaces).
pub(crate) fn color_for(id: &str) -> u32 {
    const PALETTE: [u32; 8] = [0xff3b30, 0xff9500, 0xffcc00, 0x34c759, 0x00c7be, 0x007aff, 0xaf52de, 0xff2d55];
    PALETTE[id.bytes().fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)) % PALETTE.len()]
}

fn ns_err(context: &str, err: &NSError) -> ApiError {
    ApiError::internal(format!("{context}: {}", err.localizedDescription()))
}

impl Calendar {
    pub fn new() -> Self {
        Self { store: Mutex::new(None) }
    }

    fn with_store<T>(&self, f: impl FnOnce(&EKEventStore) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let mut guard = crate::lock_within(&self.store, Duration::from_secs(15)).ok_or_else(|| {
            ApiError::internal("Calendar on the Mac isn't responding (an EventKit call is stuck). Restart bbd on the Mac.")
        })?;
        if guard.is_none() {
            let store = unsafe { EKEventStore::new() };
            let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
            if status != EKAuthorizationStatus::FullAccess {
                let (tx, rx) = mpsc::channel();
                let block = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
                    let _ = tx.send(granted.as_bool());
                });
                unsafe { store.requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&block)) };
                let granted = rx.recv_timeout(Duration::from_secs(120)).unwrap_or(false);
                if !granted {
                    return Err(ApiError::new(
                        ErrorCode::PermissionDenied,
                        "Calendar access denied. Allow bbd in System Settings → Privacy & Security → Calendars.",
                    ));
                }
            }
            *guard = Some(Store(store));
        }
        f(&guard.as_ref().unwrap().0)
    }

    pub fn calendars(&self) -> Result<Response, ApiError> {
        self.with_store(|store| {
            let list = unsafe { store.calendarsForEntityType(EKEntityType::Event) };
            Ok(Response::Calendars(
                list.iter()
                    .map(|c| {
                        let id = unsafe { c.calendarIdentifier() }.to_string();
                        Cal {
                            color: color_for(&id),
                            title: unsafe { c.title() }.to_string(),
                            writable: unsafe { c.allowsContentModifications() },
                            id,
                        }
                    })
                    .collect(),
            ))
        })
    }

    fn to_proto(event: &EKEvent) -> CalendarEvent {
        unsafe {
            CalendarEvent {
                id: event.eventIdentifier().map(|s| s.to_string()).unwrap_or_default(),
                calendar_id: event.calendar().map(|c| c.calendarIdentifier().to_string()).unwrap_or_default(),
                title: event.title().to_string(),
                location: event.location().map(|s| s.to_string()).filter(|s| !s.is_empty()),
                notes: event.notes().map(|s| s.to_string()).filter(|s| !s.is_empty()),
                start: millis(&event.startDate()),
                end: millis(&event.endDate()),
                all_day: event.isAllDay(),
            }
        }
    }

    pub fn events(&self, from: Timestamp, to: Timestamp) -> Result<Response, ApiError> {
        self.with_store(|store| {
            let predicate = unsafe { store.predicateForEventsWithStartDate_endDate_calendars(&date(from), &date(to), None) };
            let events = unsafe { store.eventsMatchingPredicate(&predicate) };
            Ok(Response::Events(events.iter().map(|e| Self::to_proto(&e)).collect()))
        })
    }

    pub fn create(&self, new: &NewEvent) -> Result<Response, ApiError> {
        self.with_store(|store| unsafe {
            let calendar: Retained<EKCalendar> = store
                .calendarWithIdentifier(&NSString::from_str(&new.calendar_id))
                .ok_or_else(|| ApiError::not_found("calendar"))?;
            let event = EKEvent::eventWithEventStore(store);
            event.setCalendar(Some(&calendar));
            event.setTitle(Some(&NSString::from_str(&new.title)));
            event.setStartDate(Some(&date(new.start)));
            event.setEndDate(Some(&date(new.end)));
            event.setAllDay(new.all_day);
            if let Some(location) = &new.location {
                event.setLocation(Some(&NSString::from_str(location)));
            }
            if let Some(notes) = &new.notes {
                event.setNotes(Some(&NSString::from_str(notes)));
            }
            store.saveEvent_span_error(&event, EKSpan::ThisEvent).map_err(|e| ns_err("saving event", &e))?;
            Ok(Response::Event(Self::to_proto(&event)))
        })
    }

    pub fn delete(&self, id: &str) -> Result<Response, ApiError> {
        self.with_store(|store| unsafe {
            let event = store.eventWithIdentifier(&NSString::from_str(id)).ok_or_else(|| ApiError::not_found("event"))?;
            store.removeEvent_span_error(&event, EKSpan::ThisEvent).map_err(|e| ns_err("deleting event", &e))?;
            Ok(Response::Ok)
        })
    }

    /// EventKit's change notification needs a run loop; a fingerprint poll is simpler
    /// and cheap: the events over the next 60 days.
    pub fn watch(&self, sink: EventSink) {
        // One long-lived store, never released (see `crate::run_main_loop`): with the main
        // run loop running it follows changes from the iPhone itself.
        let calendar = Calendar::new();
        std::thread::spawn(move || {
            let mut last: Option<u64> = None;
            loop {
                std::thread::sleep(Duration::from_secs(10));
                let now = crate::now_ms();
                let Ok(Response::Events(events)) = calendar.events(now - 86_400_000, now + 60 * 86_400_000) else { continue };
                let fingerprint = events.iter().fold(events.len() as u64, |h, e| {
                    e.id.bytes().chain(e.title.bytes()).fold(h ^ e.start as u64, |h, b| h.wrapping_mul(1_099_511_628_211) ^ u64::from(b))
                });
                if last.is_some_and(|l| l != fingerprint) {
                    sink(Event::CalendarChanged);
                }
                last = Some(fingerprint);
            }
        });
    }
}
