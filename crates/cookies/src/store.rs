//! SQLite cookie store implementation.

use chrono::Utc;
use cloudyab_types::cookie::{Cookie, CookieJar, SameSite};
use rusqlite::{params, Connection};
use std::path::Path;
use thiserror::Error;
use tracing::{debug, info};

/// Errors from cookie store operations.
#[derive(Debug, Error)]
pub enum CookieStoreError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Cookie not found for domain: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// SQLite-backed persistent cookie store.
pub struct CookieStore {
    conn: Connection,
}

impl CookieStore {
    /// Open or create a cookie store at the given path.
    pub fn open(path: &Path) -> Result<Self, CookieStoreError> {
        let conn = Connection::open(path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cookies (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                value TEXT NOT NULL,
                domain TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '/',
                expires TEXT,
                secure INTEGER NOT NULL DEFAULT 0,
                http_only INTEGER NOT NULL DEFAULT 0,
                same_site TEXT NOT NULL DEFAULT 'lax',
                source_url TEXT,
                user_agent TEXT,
                captured_at TEXT NOT NULL,
                UNIQUE(name, domain, path)
            );
            CREATE INDEX IF NOT EXISTS idx_cookies_domain ON cookies(domain);",
        )?;

        info!(path = %path.display(), "Cookie store opened");
        Ok(Self { conn })
    }

    /// Store cookies from a jar (upserts on conflict).
    pub fn store_jar(&self, jar: &CookieJar) -> Result<(), CookieStoreError> {
        let tx = self.conn.unchecked_transaction()?;

        for cookie in &jar.cookies {
            tx.execute(
                "INSERT OR REPLACE INTO cookies (name, value, domain, path, expires, secure, http_only, same_site, source_url, user_agent, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    cookie.name,
                    cookie.value,
                    cookie.domain,
                    cookie.path,
                    cookie.expires.map(|e| e.to_rfc3339()),
                    cookie.secure as i32,
                    cookie.http_only as i32,
                    format!("{:?}", cookie.same_site).to_lowercase(),
                    jar.source_url,
                    jar.user_agent,
                    jar.captured_at.to_rfc3339(),
                ],
            )?;
        }

        tx.commit()?;
        debug!(count = jar.cookies.len(), domain = %jar.source_url, "Stored cookies");
        Ok(())
    }

    /// Get all cookies for a domain.
    pub fn get_by_domain(&self, domain: &str) -> Result<Vec<Cookie>, CookieStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, value, domain, path, expires, secure, http_only, same_site
             FROM cookies WHERE domain = ?1 OR domain LIKE ?2",
        )?;

        let wildcard = format!(".{domain}");
        let cookies = stmt
            .query_map(params![domain, wildcard], |row| {
                let same_site_str: String = row.get(7)?;
                let same_site = match same_site_str.as_str() {
                    "strict" => SameSite::Strict,
                    "none" => SameSite::None,
                    _ => SameSite::Lax,
                };

                Ok(Cookie {
                    name: row.get(0)?,
                    value: row.get(1)?,
                    domain: row.get(2)?,
                    path: row.get(3)?,
                    expires: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    secure: row.get::<_, i32>(5)? != 0,
                    http_only: row.get::<_, i32>(6)? != 0,
                    same_site,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(cookies)
    }

    /// Export all cookies as a CookieJar.
    pub fn export_all(&self) -> Result<CookieJar, CookieStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, value, domain, path, expires, secure, http_only, same_site
             FROM cookies ORDER BY domain",
        )?;

        let cookies = stmt
            .query_map([], |row| {
                let same_site_str: String = row.get(7)?;
                let same_site = match same_site_str.as_str() {
                    "strict" => SameSite::Strict,
                    "none" => SameSite::None,
                    _ => SameSite::Lax,
                };

                Ok(Cookie {
                    name: row.get(0)?,
                    value: row.get(1)?,
                    domain: row.get(2)?,
                    path: row.get(3)?,
                    expires: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    secure: row.get::<_, i32>(5)? != 0,
                    http_only: row.get::<_, i32>(6)? != 0,
                    same_site,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CookieJar {
            source_url: "export".into(),
            user_agent: String::new(),
            captured_at: Utc::now(),
            cookies,
        })
    }

    /// Delete all cookies for a domain.
    pub fn delete_domain(&self, domain: &str) -> Result<usize, CookieStoreError> {
        let count = self.conn.execute(
            "DELETE FROM cookies WHERE domain = ?1 OR domain LIKE ?2",
            params![domain, format!(".{domain}")],
        )?;
        debug!(domain, count, "Deleted cookies");
        Ok(count)
    }

    /// Delete expired cookies.
    pub fn cleanup_expired(&self) -> Result<usize, CookieStoreError> {
        let now = Utc::now().to_rfc3339();
        let count = self.conn.execute(
            "DELETE FROM cookies WHERE expires IS NOT NULL AND expires < ?1",
            params![now],
        )?;
        debug!(count, "Cleaned up expired cookies");
        Ok(count)
    }
}
