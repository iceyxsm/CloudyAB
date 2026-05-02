//! Shared types and data structures for CloudyAB.
//!
//! This crate contains all DTOs, enums, and core data structures used across
//! the workspace. It has zero logic — only data definitions.

pub mod captcha;
pub mod cookie;
pub mod error;
pub mod fingerprint;
pub mod page;
pub mod session;

// Re-export key types at crate root for convenience
pub use captcha::{CaptchaResult, CaptchaSolution, CaptchaType};
pub use cookie::{Cookie, CookieJar, SameSite};
pub use page::{ElementRef, PageSnapshot, SnapshotOptions};
pub use session::{Layer, NavigationResult, ProxyConfig, SessionConfig};
