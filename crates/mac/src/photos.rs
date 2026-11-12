//! Photos via PhotoKit. Images are rendered by PhotoKit (orientation applied, iCloud
//! originals downloaded if needed) and re-encoded as JPEG, so clients never see HEIC.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use bb_proto::{Album, ApiError, Asset, ErrorCode, Event, EventSink, Response};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSArray, NSDictionary, NSSortDescriptor, NSString};
use objc2_photos::{
    PHAccessLevel, PHAsset, PHAssetCollection, PHAssetCollectionSubtype, PHAssetCollectionType, PHAssetMediaType,
    PHAuthorizationStatus, PHFetchOptions, PHFetchResult, PHImageContentMode, PHImageManager, PHImageRequestOptions,
    PHImageRequestOptionsDeliveryMode, PHImageRequestOptionsResizeMode, PHPhotoLibrary,
};

/// Album id for "everything, newest first".
const ALL: &str = "all";

pub struct Photos {
    /// Encoded JPEGs by (asset id, size), so chunked reads don't re-render.
    cache: Mutex<HashMap<(String, u32), std::sync::Arc<Vec<u8>>>>,
}

fn ensure_access() -> Result<(), ApiError> {
    let status = unsafe { PHPhotoLibrary::authorizationStatusForAccessLevel(PHAccessLevel::ReadWrite) };
    if status == PHAuthorizationStatus::Authorized || status == PHAuthorizationStatus::Limited {
        return Ok(());
    }
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |status: PHAuthorizationStatus| {
        let _ = tx.send(status);
    });
    unsafe { PHPhotoLibrary::requestAuthorizationForAccessLevel_handler(PHAccessLevel::ReadWrite, &block) };
    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(s) if s == PHAuthorizationStatus::Authorized || s == PHAuthorizationStatus::Limited => Ok(()),
        _ => Err(ApiError::new(
            ErrorCode::PermissionDenied,
            "Photos access denied. Allow bbd in System Settings → Privacy & Security → Photos.",
        )),
    }
}

fn newest_first() -> Retained<PHFetchOptions> {
    unsafe {
        let options = PHFetchOptions::new();
        let sort = NSSortDescriptor::sortDescriptorWithKey_ascending(Some(&NSString::from_str("creationDate")), false);
        options.setSortDescriptors(Some(&NSArray::from_retained_slice(&[sort])));
        options
    }
}

fn collection(id: &str) -> Option<Retained<PHAssetCollection>> {
    let ids = NSArray::from_retained_slice(&[NSString::from_str(id)]);
    let result = unsafe { PHAssetCollection::fetchAssetCollectionsWithLocalIdentifiers_options(&ids, None) };
    (unsafe { result.count() } > 0).then(|| unsafe { result.objectAtIndex(0) })
}

fn assets_in(album: Option<&str>) -> Result<Retained<PHFetchResult<PHAsset>>, ApiError> {
    let options = newest_first();
    Ok(match album {
        None | Some(ALL) => unsafe { PHAsset::fetchAssetsWithOptions(Some(&options)) },
        Some(id) => {
            let c = collection(id).ok_or_else(|| ApiError::not_found("album"))?;
            unsafe { PHAsset::fetchAssetsInAssetCollection_options(&c, Some(&options)) }
        }
    })
}

fn asset(id: &str) -> Result<Retained<PHAsset>, ApiError> {
    let ids = NSArray::from_retained_slice(&[NSString::from_str(id)]);
    let result = unsafe { PHAsset::fetchAssetsWithLocalIdentifiers_options(&ids, None) };
    if unsafe { result.count() } == 0 {
        return Err(ApiError::not_found("photo"));
    }
    Ok(unsafe { result.objectAtIndex(0) })
}

/// Render through PhotoKit at up to `max_side` pixels and encode JPEG.
fn render_jpeg(asset: &PHAsset, max_side: f64) -> Result<Vec<u8>, ApiError> {
    let out: std::rc::Rc<RefCell<Option<Retained<NSImage>>>> = Default::default();
    unsafe {
        let options = PHImageRequestOptions::new();
        options.setSynchronous(true);
        options.setNetworkAccessAllowed(true);
        options.setDeliveryMode(PHImageRequestOptionsDeliveryMode::HighQualityFormat);
        options.setResizeMode(PHImageRequestOptionsResizeMode::Fast);
        let slot = out.clone();
        let handler = RcBlock::new(move |image: *mut NSImage, _info: *mut NSDictionary| {
            if let Some(image) = Retained::retain(image) {
                *slot.borrow_mut() = Some(image);
            }
        });
        let (w, h) = (asset.pixelWidth() as f64, asset.pixelHeight() as f64);
        let scale = (max_side / w.max(h).max(1.0)).min(1.0);
        PHImageManager::defaultManager().requestImageForAsset_targetSize_contentMode_options_resultHandler(
            asset,
            CGSize { width: (w * scale).max(1.0), height: (h * scale).max(1.0) },
            PHImageContentMode::AspectFit,
            Some(&options),
            &handler,
        );
    }
    let image = out.borrow_mut().take().ok_or_else(|| ApiError::internal("Photos returned no image"))?;
    let tiff = image.TIFFRepresentation().ok_or_else(|| ApiError::internal("could not read image"))?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff).ok_or_else(|| ApiError::internal("could not decode image"))?;
    let jpeg = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &NSDictionary::new()) }
        .ok_or_else(|| ApiError::internal("could not encode JPEG"))?;
    Ok(jpeg.to_vec())
}

impl Photos {
    pub fn new() -> Self {
        Self { cache: Mutex::new(HashMap::new()) }
    }

    pub fn albums(&self) -> Result<Response, ApiError> {
        ensure_access()?;
        let mut albums = vec![Album {
            id: ALL.into(),
            title: "Library".into(),
            count: unsafe { PHAsset::fetchAssetsWithOptions(None).count() } as u32,
        }];
        let kinds = [
            (PHAssetCollectionType::SmartAlbum, PHAssetCollectionSubtype::SmartAlbumFavorites),
            (PHAssetCollectionType::Album, PHAssetCollectionSubtype::Any),
        ];
        for (kind, subtype) in kinds {
            let result = unsafe { PHAssetCollection::fetchAssetCollectionsWithType_subtype_options(kind, subtype, None) };
            for i in 0..unsafe { result.count() } {
                let c = unsafe { result.objectAtIndex(i) };
                let count = unsafe { PHAsset::fetchAssetsInAssetCollection_options(&c, None).count() } as u32;
                albums.push(Album {
                    id: unsafe { c.localIdentifier() }.to_string(),
                    title: unsafe { c.localizedTitle() }.map(|t| t.to_string()).unwrap_or_else(|| "Album".into()),
                    count,
                });
            }
        }
        Ok(Response::Albums(albums))
    }

    pub fn assets(&self, album: Option<&str>, offset: u32, limit: u32) -> Result<Response, ApiError> {
        ensure_access()?;
        let result = assets_in(album)?;
        let total = unsafe { result.count() };
        let end = (offset as usize + limit as usize).min(total);
        let assets = (offset as usize..end)
            .map(|i| unsafe {
                let a = result.objectAtIndex(i);
                Asset {
                    id: a.localIdentifier().to_string(),
                    created: a.creationDate().map(|d| (d.timeIntervalSince1970() * 1000.0) as i64).unwrap_or(0),
                    width: a.pixelWidth() as u32,
                    height: a.pixelHeight() as u32,
                    is_video: a.mediaType() == PHAssetMediaType::Video,
                    favorite: a.isFavorite(),
                }
            })
            .collect();
        Ok(Response::Assets(assets))
    }

    pub fn image(&self, id: &str, max_side: f64, offset: u64, len: u32) -> Result<Response, ApiError> {
        ensure_access()?;
        let key = (id.to_string(), max_side as u32);
        let cached = self.cache.lock().unwrap().get(&key).cloned();
        let data = match cached {
            Some(data) => data,
            None => {
                let asset = asset(id)?;
                let data = std::sync::Arc::new(render_jpeg(&asset, max_side)?);
                let mut cache = self.cache.lock().unwrap();
                if cache.len() > 512 {
                    cache.clear();
                }
                cache.insert(key, data.clone());
                data
            }
        };
        let total = data.len() as u64;
        let start = offset.min(total) as usize;
        let end = (offset + u64::from(len.min(bb_proto::MAX_BLOB_CHUNK))).min(total) as usize;
        Ok(Response::Blob { data: data[start..end].to_vec(), total, mime_type: Some("image/jpeg".into()) })
    }

    /// Change observers need an Objective-C class; polling the library count is enough
    /// to tell clients to refresh.
    pub fn watch(&self, sink: EventSink) {
        std::thread::spawn(move || {
            let mut last = None;
            loop {
                std::thread::sleep(Duration::from_secs(30));
                if ensure_access().is_err() {
                    continue;
                }
                let count = unsafe { PHAsset::fetchAssetsWithOptions(None).count() };
                if last.is_some_and(|l| l != count) {
                    sink(Event::PhotosChanged);
                }
                last = Some(count);
            }
        });
    }
}
