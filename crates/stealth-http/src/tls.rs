//! TLS fingerprint configuration for JA3/JA4 spoofing.

use cloudyab_types::fingerprint::TlsProfile;

/// Pre-built TLS profiles that mimic real browsers.
pub struct TlsProfiles;

impl TlsProfiles {
    /// Chrome 125 on Windows 11 TLS fingerprint.
    pub fn chrome_125_win11() -> TlsProfile {
        TlsProfile {
            cipher_suites: vec![
                "TLS_AES_128_GCM_SHA256".into(),
                "TLS_AES_256_GCM_SHA384".into(),
                "TLS_CHACHA20_POLY1305_SHA256".into(),
                "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384".into(),
                "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384".into(),
                "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256".into(),
                "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256".into(),
            ],
            extensions: vec![
                "server_name".into(),
                "extended_master_secret".into(),
                "renegotiation_info".into(),
                "supported_groups".into(),
                "ec_point_formats".into(),
                "session_ticket".into(),
                "application_layer_protocol_negotiation".into(),
                "status_request".into(),
                "signature_algorithms".into(),
                "signed_certificate_timestamp".into(),
                "key_share".into(),
                "psk_key_exchange_modes".into(),
                "supported_versions".into(),
                "compress_certificate".into(),
                "application_settings".into(),
            ],
            elliptic_curves: vec!["X25519".into(), "P-256".into(), "P-384".into()],
            ec_point_formats: vec!["uncompressed".into()],
        }
    }

    /// Firefox 126 on Windows 11 TLS fingerprint.
    pub fn firefox_126_win11() -> TlsProfile {
        TlsProfile {
            cipher_suites: vec![
                "TLS_AES_128_GCM_SHA256".into(),
                "TLS_CHACHA20_POLY1305_SHA256".into(),
                "TLS_AES_256_GCM_SHA384".into(),
                "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256".into(),
                "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256".into(),
                "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384".into(),
                "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384".into(),
                "TLS_RSA_WITH_AES_128_GCM_SHA256".into(),
                "TLS_RSA_WITH_AES_256_GCM_SHA384".into(),
            ],
            extensions: vec![
                "server_name".into(),
                "extended_master_secret".into(),
                "renegotiation_info".into(),
                "supported_groups".into(),
                "ec_point_formats".into(),
                "session_ticket".into(),
                "application_layer_protocol_negotiation".into(),
                "status_request".into(),
                "delegated_credentials".into(),
                "key_share".into(),
                "supported_versions".into(),
                "signature_algorithms".into(),
                "psk_key_exchange_modes".into(),
                "record_size_limit".into(),
            ],
            elliptic_curves: vec![
                "X25519".into(),
                "P-256".into(),
                "P-384".into(),
                "P-521".into(),
                "ffdhe2048".into(),
                "ffdhe3072".into(),
            ],
            ec_point_formats: vec!["uncompressed".into()],
        }
    }
}
