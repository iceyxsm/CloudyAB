//! Adapter that bridges the core `CookiePersistence` trait to the cookies crate's `CookieStore`.

use cloudyab_cookies::store::CookieStore;
use cloudyab_core::engine::{CookiePersistence, EngineError};
use cloudyab_types::cookie::{Cookie, CookieJar};
use std::path::Path;
use std::sync::Mutex;
use tracing::info;

/// Adapter wrapping the SQLite CookieStore for the orchestrator.
pub struct CookieStoreAdapter {
    store: Mutex<CookieStore>,
}

impl CookieStoreAdapter {
    /// Open or create the cookie store at the given path.
    pub fn open(path: &Path) -> Result<Self, EngineError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EngineError::CookieError(format!("Failed to create cookie db directory: {e}"))
            })?;
        }

        let store = CookieStore::open(path)
            .map_err(|e| EngineError::CookieError(format!("Failed to open cookie store: {e}")))?;

        info!(path = %path.display(), "Cookie persistence store opened");
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

impl CookiePersistence for CookieStoreAdapter {
    fn persist(&self, jar: &CookieJar) -> Result<(), EngineError> {
        let store = self
            .store
            .lock()
            .map_err(|e| EngineError::CookieError(format!("Cookie store lock poisoned: {e}")))?;
        store
            .store_jar(jar)
            .map_err(|e| EngineError::CookieError(format!("Failed to persist cookies: {e}")))
    }

    fn load_for_domain(&self, domain: &str) -> Result<Vec<Cookie>, EngineError> {
        let store = self
            .store
            .lock()
            .map_err(|e| EngineError::CookieError(format!("Cookie store lock poisoned: {e}")))?;
        store
            .get_by_domain(domain)
            .map_err(|e| EngineError::CookieError(format!("Failed to load cookies: {e}")))
    }
}
