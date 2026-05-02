//! Cookie types for storage and exchange.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A browser cookie with all standard attributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    /// Cookie name
    pub name: String,
    /// Cookie value
    pub value: String,
    /// Domain the cookie belongs to
    pub domain: String,
    /// Path scope
    pub path: String,
    /// Expiration time (None = session cookie)
    pub expires: Option<DateTime<Utc>>,
    /// Secure flag (HTTPS only)
    pub secure: bool,
    /// HttpOnly flag (not accessible via JS)
    pub http_only: bool,
    /// SameSite attribute
    pub same_site: SameSite,
}

/// SameSite cookie attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

/// A collection of cookies for a specific domain/session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieJar {
    /// The URL these cookies were extracted from
    pub source_url: String,
    /// User-agent string used when these cookies were obtained
    pub user_agent: String,
    /// Timestamp when cookies were captured
    pub captured_at: DateTime<Utc>,
    /// The cookies themselves
    pub cookies: Vec<Cookie>,
}
