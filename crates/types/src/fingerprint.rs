//! Browser and TLS fingerprint profiles.

use serde::{Deserialize, Serialize};

/// A complete browser fingerprint profile for stealth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintProfile {
    /// Profile name (e.g., "windows_chrome_125")
    pub name: String,
    /// Operating system to emulate
    pub os: OsProfile,
    /// TLS configuration
    pub tls: TlsProfile,
    /// Browser navigator properties
    pub navigator: NavigatorProfile,
    /// Screen/viewport dimensions
    pub screen: ScreenProfile,
    /// WebGL renderer info
    pub webgl: WebGlProfile,
}

/// Operating system identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsProfile {
    pub platform: String,
    pub os_cpu: String,
    pub architecture: String,
}

/// TLS fingerprint configuration (JA3/JA4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsProfile {
    /// Ordered cipher suites
    pub cipher_suites: Vec<String>,
    /// TLS extensions in order
    pub extensions: Vec<String>,
    /// Supported elliptic curves
    pub elliptic_curves: Vec<String>,
    /// EC point formats
    pub ec_point_formats: Vec<String>,
}

/// Navigator object properties to spoof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigatorProfile {
    pub user_agent: String,
    pub platform: String,
    pub language: String,
    pub languages: Vec<String>,
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub vendor: String,
    pub max_touch_points: u32,
}

/// Screen dimensions for viewport emulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenProfile {
    pub width: u32,
    pub height: u32,
    pub avail_width: u32,
    pub avail_height: u32,
    pub color_depth: u32,
    pub pixel_ratio: f64,
}

/// WebGL renderer/vendor spoofing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebGlProfile {
    pub vendor: String,
    pub renderer: String,
}
