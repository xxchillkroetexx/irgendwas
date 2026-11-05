//! Environment parsing (`app::server::Config`).

use app::i18n::Locale;
use app::server::Config;
use app::server::config::{ConfigError, MailEncryption};

fn config(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
    let vars: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Config::from_lookup(move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
}

#[test]
fn defaults() {
    let c = config(&[("DB_PASSWORD", "pw")]).unwrap();
    assert_eq!(c.app_url, "http://localhost:3000");
    assert!(!c.app_debug);
    assert_eq!(c.app_locale, Locale::En);
    assert_eq!(c.app_timezone, chrono_tz::Europe::Berlin);
    assert_eq!(c.inactivity_limit, 7_776_000);
    assert!(c.trusted_proxies.is_empty());
    assert_eq!((c.db.host.as_str(), c.db.port), ("localhost", 3306));
    assert_eq!(c.mail.encryption, MailEncryption::StartTls);
    assert!(!c.secure_cookies());
    // Secrets never appear in Debug output.
    assert!(!format!("{c:?}").contains("pw\""));
}

#[test]
fn explicit_values() {
    let c = config(&[
        ("APP_URL", "https://wichteln.example/"),
        ("APP_DEBUG", "true"),
        ("APP_LOCALE", "de"),
        ("APP_TIMEZONE", "America/New_York"),
        ("INACTIVITY_LIMIT", "600"),
        ("TRUSTED_PROXIES_CIDR", "172.18.0.0/16, 10.0.0.1"),
        ("DB_HOST", "mariadb"),
        ("DB_PORT", "3307"),
        ("DB_PASSWORD", "pw"),
        ("MAIL_PORT", "465"),
        ("MAIL_ENCRYPTION", "ssl"),
        ("MAIL_USERNAME", ""),
    ])
    .unwrap();
    assert_eq!(c.app_url, "https://wichteln.example");
    assert!(c.app_debug && c.secure_cookies());
    assert_eq!(c.app_locale, Locale::De);
    assert_eq!(c.app_timezone, chrono_tz::America::New_York);
    assert_eq!(c.inactivity_limit, 600);
    assert_eq!(c.trusted_proxies.len(), 2);
    assert_eq!(c.db.port, 3307);
    assert_eq!(c.mail.encryption, MailEncryption::Ssl);
    assert_eq!(c.mail.username, None);
}

#[test]
fn errors() {
    assert_eq!(
        config(&[]).unwrap_err(),
        ConfigError::Missing("DB_PASSWORD")
    );
    for (name, value) in [
        ("APP_URL", "localhost"),
        ("APP_DEBUG", "maybe"),
        ("APP_LOCALE", "fr"),
        ("APP_TIMEZONE", "Mars/Olympus"),
        ("INACTIVITY_LIMIT", "-1"),
        ("TRUSTED_PROXIES_CIDR", "not-a-cidr"),
        ("DB_PORT", "99999"),
        ("MAIL_ENCRYPTION", "rot13"),
    ] {
        let err = config(&[("DB_PASSWORD", "pw"), (name, value)]).unwrap_err();
        assert!(
            matches!(err, ConfigError::Invalid { name: n, .. } if n == name),
            "{name}: {err}"
        );
    }
}

#[test]
fn trusted_proxies_accept_spaces_and_commas() {
    let c = config(&[
        ("DB_PASSWORD", "pw"),
        (
            "TRUSTED_PROXIES_CIDR",
            "172.31.250.10/32 10.0.0.0/8,192.168.1.1  , fd00::/8",
        ),
    ])
    .unwrap();
    let nets: Vec<String> = c.trusted_proxies.iter().map(ToString::to_string).collect();
    assert_eq!(
        nets,
        [
            "172.31.250.10/32",
            "10.0.0.0/8",
            "192.168.1.1/32",
            "fd00::/8"
        ]
    );
}

#[test]
fn mail_per_ip_defaults_to_20_and_accepts_positive_numbers() {
    assert_eq!(config(&[("DB_PASSWORD", "pw")]).unwrap().mail_per_ip, 20);
    let c = config(&[("DB_PASSWORD", "pw"), ("RATE_LIMIT_MAIL_PER_IP", "1000")]).unwrap();
    assert_eq!(c.mail_per_ip, 1000);
}

#[test]
fn mail_per_ip_rejects_zero_negative_and_garbage() {
    for bad in ["0", "-1", "abc", "1.5", "99999999999"] {
        let result = config(&[("DB_PASSWORD", "pw"), ("RATE_LIMIT_MAIL_PER_IP", bad)]);
        assert!(result.is_err(), "{bad:?} must be a config error");
    }
}
