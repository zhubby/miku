use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use miku_core::MikuError;

pub(crate) fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}

pub(crate) fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

pub(crate) fn to_storage_error(error: impl std::error::Error) -> MikuError {
    MikuError::Storage(error.to_string())
}
