//! Typed configuration loaded from environment variables.
//!
//! Only `DB_PASSWORD` is required; everything else
//! has a development default. `MAIL_*` is parsed here, mail sending comes later.

use std::fmt;
use std::path::Path;

use chrono_tz::Tz;
use ipnet::IpNet;

use crate::i18n::Locale;
use crate::tz::DEFAULT_TIMEZONE;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("environment variable {0} is required")]
    Missing(&'static str),
    #[error("environment variable {name} has an invalid value {value:?}: {reason}")]
    Invalid {
        name: &'static str,
        value: String,
        reason: String,
    },
}

#[derive(Clone)]
pub struct Config {
    /// Public base URL without trailing slash, e.g. `https://wichteln.example`.
    pub app_url: String,
    pub app_debug: bool,
    /// Default locale when neither session nor cookie selects one.
    pub app_locale: Locale,
    /// Zone for displaying and entering dates (`APP_TIMEZONE`).
    pub app_timezone: Tz,
    /// Session inactivity timeout in seconds (`INACTIVITY_LIMIT`).
    pub inactivity_limit: u64,
    /// Reverse proxies whose `X-Forwarded-For` is trusted (`TRUSTED_PROXIES_CIDR`,
    /// separated by spaces and/or commas).
    pub trusted_proxies: Vec<IpNet>,
    /// Registration and password-reset calls per client IP and hour
    /// (`RATE_LIMIT_MAIL_PER_IP`, default 20). Raised only for the e2e/dev server, whose
    /// whole test suite comes from one IP.
    pub mail_per_ip: u32,
    pub db: DbConfig,
    pub mail: MailConfig,
}

#[derive(Clone)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailEncryption {
    /// Implicit TLS (`ssl`, usually port 465).
    Ssl,
    /// STARTTLS (`tls`, usually port 587).
    StartTls,
    /// Plain SMTP (`none`, e.g. Mailpit).
    None,
}

#[derive(Clone)]
pub struct MailConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub encryption: MailEncryption,
    pub from_address: String,
    pub from_name: String,
}

impl Config {
    /// Reads the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads configuration through `lookup` (used by tests).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let app_url = get("APP_URL")
            .unwrap_or_else(|| "http://localhost:3000".into())
            .trim_end_matches('/')
            .to_string();
        if !(app_url.starts_with("http://") || app_url.starts_with("https://")) {
            return Err(invalid(
                "APP_URL",
                &app_url,
                "must start with http:// or https://",
            ));
        }

        let app_debug = match get("APP_DEBUG") {
            None => false,
            Some(v) => {
                parse_bool(&v).ok_or_else(|| invalid("APP_DEBUG", &v, "expected true or false"))?
            }
        };

        let app_locale = match get("APP_LOCALE") {
            None => Locale::En,
            Some(v) => Locale::from_code(&v)
                .ok_or_else(|| invalid("APP_LOCALE", &v, "expected en or de"))?,
        };

        let app_timezone = match get("APP_TIMEZONE") {
            None => DEFAULT_TIMEZONE,
            Some(v) => v
                .parse::<Tz>()
                .map_err(|_| invalid("APP_TIMEZONE", &v, "unknown IANA time zone"))?,
        };

        let inactivity_limit = match get("INACTIVITY_LIMIT") {
            None => 1200,
            Some(v) => v.parse::<u64>().ok().filter(|n| *n > 0).ok_or_else(|| {
                invalid(
                    "INACTIVITY_LIMIT",
                    &v,
                    "expected a positive number of seconds",
                )
            })?,
        };

        let mail_per_ip = match get("RATE_LIMIT_MAIL_PER_IP") {
            None => crate::server::rate_limit::MAIL_PER_IP.max,
            Some(v) => v.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
                invalid("RATE_LIMIT_MAIL_PER_IP", &v, "expected a positive number")
            })?,
        };

        let trusted_proxies = match get("TRUSTED_PROXIES_CIDR") {
            None => Vec::new(),
            Some(v) => v
                .split(|c: char| c == ',' || c.is_whitespace())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.parse::<IpNet>()
                        .or_else(|_| s.parse::<std::net::IpAddr>().map(IpNet::from))
                        .map_err(|_| {
                            invalid(
                                "TRUSTED_PROXIES_CIDR",
                                s,
                                "expected CIDR like 172.18.0.0/16",
                            )
                        })
                })
                .collect::<Result<_, _>>()?,
        };

        let db = DbConfig {
            host: get("DB_HOST").unwrap_or_else(|| "localhost".into()),
            port: parse_port(&get, "DB_PORT", 3306)?,
            database: get("DB_DATABASE").unwrap_or_else(|| "irgendwas_db".into()),
            username: get("DB_USERNAME").unwrap_or_else(|| "irgendjemand".into()),
            password: lookup("DB_PASSWORD").ok_or(ConfigError::Missing("DB_PASSWORD"))?,
        };

        let mail_port = parse_port(&get, "MAIL_PORT", 587)?;
        let encryption = match get("MAIL_ENCRYPTION").map(|v| v.to_ascii_lowercase()) {
            None => match mail_port {
                465 => MailEncryption::Ssl,
                587 => MailEncryption::StartTls,
                _ => MailEncryption::None,
            },
            Some(v) => match v.as_str() {
                "ssl" | "smtps" => MailEncryption::Ssl,
                "tls" | "starttls" => MailEncryption::StartTls,
                "none" | "plain" => MailEncryption::None,
                _ => return Err(invalid("MAIL_ENCRYPTION", &v, "expected ssl, tls or none")),
            },
        };
        let mail = MailConfig {
            host: get("MAIL_HOST").unwrap_or_else(|| "localhost".into()),
            port: mail_port,
            username: get("MAIL_USERNAME"),
            password: get("MAIL_PASSWORD"),
            encryption,
            from_address: get("MAIL_FROM_ADDRESS").unwrap_or_else(|| "noreply@example.com".into()),
            from_name: get("MAIL_FROM_NAME").unwrap_or_else(|| "Secret Santa".into()),
        };

        Ok(Config {
            app_url,
            app_debug,
            app_locale,
            app_timezone,
            inactivity_limit,
            trusted_proxies,
            mail_per_ip,
            db,
            mail,
        })
    }

    /// Session cookies get the `Secure` flag when the public URL is https.
    pub fn secure_cookies(&self) -> bool {
        self.app_url.starts_with("https://")
    }

    /// The web origin of `APP_URL` (`scheme://host[:port]`, lowercase, default port
    /// omitted), as browsers send it in the `Origin` header. Used by the CSRF check.
    pub fn app_origin(&self) -> String {
        normalize_origin(&self.app_url).unwrap_or_else(|| self.app_url.clone())
    }
}

/// Normalises a URL or an `Origin` header value to `scheme://host[:port]`: lowercase,
/// path/query dropped, default port (80 for http, 443 for https) removed. Returns `None`
/// for anything that is not an http(s) URL with a host (including the opaque `null`).
pub fn normalize_origin(value: &str) -> Option<String> {
    let value = value.trim();
    let (scheme, rest) = value.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let default_port: u16 = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let authority = authority.to_ascii_lowercase();
    // Split off a port, taking care of bracketed IPv6 literals (`[::1]:3000`).
    let (host, port) = match authority.rfind(':') {
        Some(i) if !authority[i..].contains(']') => (&authority[..i], Some(&authority[i + 1..])),
        _ => (authority.as_str(), None),
    };
    if host.is_empty() {
        return None;
    }
    match port.map(str::parse::<u16>) {
        None => Some(format!("{scheme}://{host}")),
        Some(Err(_)) => None,
        Some(Ok(p)) if p == default_port => Some(format!("{scheme}://{host}")),
        Some(Ok(p)) => Some(format!("{scheme}://{host}:{p}")),
    }
}

/// Loads `./.env` (current directory only) if it exists; never walks up the directory tree.
/// Variables already present in the environment win.
pub fn load_dotenv() -> Result<bool, dotenvy::Error> {
    let path = Path::new(".env");
    if path.is_file() {
        dotenvy::from_path(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn invalid(name: &'static str, value: &str, reason: &str) -> ConfigError {
    ConfigError::Invalid {
        name,
        value: value.to_string(),
        reason: reason.to_string(),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_port(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: u16,
) -> Result<u16, ConfigError> {
    match get(name) {
        None => Ok(default),
        Some(v) => v
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| invalid(name, &v, "expected a port number")),
    }
}

// Manual Debug impls so secrets never end up in logs.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("app_url", &self.app_url)
            .field("app_debug", &self.app_debug)
            .field("app_locale", &self.app_locale)
            .field("app_timezone", &self.app_timezone)
            .field("inactivity_limit", &self.inactivity_limit)
            .field("trusted_proxies", &self.trusted_proxies)
            .field("mail_per_ip", &self.mail_per_ip)
            .field("db", &self.db)
            .field("mail", &self.mail)
            .finish()
    }
}

impl fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

impl fmt::Debug for MailConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("encryption", &self.encryption)
            .field("from_address", &self.from_address)
            .field("from_name", &self.from_name)
            .finish()
    }
}
