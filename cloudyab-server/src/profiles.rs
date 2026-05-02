//! Fingerprint profile loading from TOML files.
//!
//! Loads browser fingerprint profiles from a directory of TOML files.
//! Each file defines a complete fingerprint (user-agent, TLS, screen, WebGL, etc.).
//! If no profiles directory exists, falls back to a built-in default.

use cloudyab_types::fingerprint::{
    FingerprintProfile, NavigatorProfile, OsProfile, ScreenProfile, TlsProfile, WebGlProfile,
};
use std::path::Path;
use tracing::{info, warn};

/// Load a fingerprint profile by name from the profiles directory.
/// Falls back to the built-in default if the file doesn't exist.
#[allow(dead_code)]
pub fn load_profile(profiles_dir: &Path, name: &str) -> FingerprintProfile {
    let file_path = profiles_dir.join(format!("{name}.toml"));

    if file_path.exists() {
        match load_from_file(&file_path) {
            Ok(profile) => {
                info!(name, "Loaded fingerprint profile from file");
                return profile;
            }
            Err(e) => {
                warn!(name, error = %e, "Failed to load profile, using default");
            }
        }
    }

    info!("Using built-in default fingerprint profile (Chrome 125 / Windows 11)");
    builtin_chrome_125()
}

/// Load a random profile from the profiles directory.
/// If directory is empty or doesn't exist, returns the built-in default.
pub fn load_random_profile(profiles_dir: &Path) -> FingerprintProfile {
    if !profiles_dir.exists() {
        return builtin_chrome_125();
    }

    let entries: Vec<_> = std::fs::read_dir(profiles_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .collect();

    if entries.is_empty() {
        return builtin_chrome_125();
    }

    let idx = rand::random::<usize>() % entries.len();
    let path = entries[idx].path();

    match load_from_file(&path) {
        Ok(profile) => {
            info!(profile = %profile.name, "Loaded random fingerprint profile");
            profile
        }
        Err(e) => {
            warn!(error = %e, "Failed to load random profile, using default");
            builtin_chrome_125()
        }
    }
}

/// Load a fingerprint profile from a TOML file.
fn load_from_file(path: &Path) -> Result<FingerprintProfile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Read failed: {e}"))?;
    toml::from_str(&content)
        .map_err(|e| format!("Parse failed: {e}"))
}

/// Built-in default: Chrome 125 on Windows 11.
pub fn builtin_chrome_125() -> FingerprintProfile {
    FingerprintProfile {
        name: "windows_chrome_125".into(),
        os: OsProfile {
            platform: "Windows".into(),
            os_cpu: "Windows NT 10.0; Win64; x64".into(),
            architecture: "x86_64".into(),
        },
        tls: TlsProfile {
            cipher_suites: vec![
                "TLS_AES_128_GCM_SHA256".into(),
                "TLS_AES_256_GCM_SHA384".into(),
                "TLS_CHACHA20_POLY1305_SHA256".into(),
                "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".into(),
            ],
            extensions: vec![
                "server_name".into(),
                "supported_groups".into(),
                "key_share".into(),
                "supported_versions".into(),
            ],
            elliptic_curves: vec!["X25519".into(), "P-256".into(), "P-384".into()],
            ec_point_formats: vec!["uncompressed".into()],
        },
        navigator: NavigatorProfile {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36".into(),
            platform: "Win32".into(),
            language: "en-US".into(),
            languages: vec!["en-US".into(), "en".into()],
            hardware_concurrency: 8,
            device_memory: 8,
            vendor: "Google Inc.".into(),
            max_touch_points: 0,
        },
        screen: ScreenProfile {
            width: 1920,
            height: 1080,
            avail_width: 1920,
            avail_height: 1040,
            color_depth: 24,
            pixel_ratio: 1.0,
        },
        webgl: WebGlProfile {
            vendor: "Google Inc. (NVIDIA)".into(),
            renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)".into(),
        },
    }
}
