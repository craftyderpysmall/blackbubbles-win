//! Your Apple devices (and optionally your family's) from iCloud Find My.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bb_findmy::icloud;
use bb_proto::{ApiError, ErrorCode, FindMyDevice, Location, Response};

const DEVICE_TTL: Duration = Duration::from_secs(60);

pub struct FindMy {
    dir: PathBuf,
    session: Mutex<Option<icloud::Session>>,
    /// Keyed by `include_family`, so switching the setting never shows a stale list.
    cache: Mutex<[Option<(Instant, Vec<FindMyDevice>)>; 2]>,
}

impl FindMy {
    pub fn new(data_dir: &Path) -> Self {
        Self { dir: data_dir.to_path_buf(), session: Mutex::new(None), cache: Mutex::default() }
    }

    fn fetch(&self, include_family: bool) -> Result<Vec<FindMyDevice>, String> {
        let mut guard = self.session.lock().unwrap();
        if guard.is_none() {
            *guard = Some(icloud::Session::open(&self.dir.join("icloud")).map_err(|e| e.to_string())?);
        }
        let session = guard.as_mut().unwrap();
        if !session.is_signed_in() {
            return Err("Find My is not signed in; run `bbd findmy-login <apple id>` on the Mac".into());
        }
        let devices = match session.devices(include_family) {
            Ok(devices) => devices,
            Err(err) => {
                // Reload from disk next time, so a `bbd findmy-login` run meanwhile takes effect
                // without restarting bbd.
                *guard = None;
                return Err(format!("{err:#}"));
            }
        };
        Ok(devices
            .into_iter()
            .map(|d| FindMyDevice {
                id: d.id,
                name: d.name,
                model: d.model,
                battery: d.battery,
                location: d.location.map(|(lat, lon, accuracy_m, at)| Location { lat, lon, accuracy_m, at }),
            })
            .collect())
    }

    pub fn list(&self, include_family: bool) -> Result<Response, ApiError> {
        let slot = usize::from(include_family);
        if let Some((at, list)) = &self.cache.lock().unwrap()[slot]
            && at.elapsed() < DEVICE_TTL
        {
            return Ok(Response::FindMyDevices(list.clone()));
        }
        let list = self.fetch(include_family).map_err(|e| ApiError::new(ErrorCode::Unsupported, e))?;
        self.cache.lock().unwrap()[slot] = Some((Instant::now(), list.clone()));
        Ok(Response::FindMyDevices(list))
    }
}
