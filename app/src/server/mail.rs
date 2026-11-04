//! Outgoing mail over SMTP (`lettre`, rustls).
//!
//! The [`Mailer`] lives on [`AppState`](super::AppState). [`Mailer::send`] never blocks the
//! request: it hands the message to a spawned task and returns a [`MailHandle`] at once.
//! Callers that don't care about delivery drop the handle (the mail is still sent); callers
//! that do (e.g. the draw slice's `notification_sent`) `.await` [`MailHandle::outcome`].
//!
//! ```ignore
//! let mailer = app::server::state()?.mailer;
//! mailer.send(Message {
//!     to: user.email.clone(),
//!     subject: i18n.t("email.reset.subject"),
//!     html: format!("<p><a href=\"{0}\">{0}</a></p>", escape_html(&link)),
//!     text: link,
//! });
//! ```
//!
//! Failures are logged with the recipient's domain and the error, never with the subject,
//! body, full address, or any token.
//!
//! Transport by `MAIL_ENCRYPTION` (see [`MailEncryption`]): `ssl` = implicit TLS (port 465),
//! `tls` = STARTTLS (port 587, required, no plaintext fallback), `none` = plain SMTP (Mailpit
//! in development and tests only).

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use super::config::{MailConfig, MailEncryption};

/// At most this many SMTP sessions run at the same time; further mails wait their turn.
const MAX_CONCURRENT_SENDS: usize = 4;
/// Per-command SMTP timeout.
const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

/// One outgoing mail. `html` must already be safe HTML: escape every interpolated value
/// with [`escape_html`]. `text` is the plain-text alternative.
#[derive(Clone)]
pub struct Message {
    pub to: String,
    pub subject: String,
    pub html: String,
    pub text: String,
}

// Bodies may contain tokens: keep them out of debug output.
impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("to", &recipient_domain(&self.to))
            .finish_non_exhaustive()
    }
}

/// A mail failure. Its texts are safe to log: they never carry an SMTP server reply (which
/// can quote the recipient's address) or a part of the message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailError {
    /// The recipient (or the configured sender) is not a valid address.
    #[error("invalid address")]
    Address,
    /// The message could not be assembled.
    #[error("could not build message: {0}")]
    Build(String),
    /// The SMTP server could not be reached or refused the message. The text is a kind and
    /// the SMTP reply code only (see [`smtp_error`]), never the reply text.
    #[error("smtp delivery failed: {0}")]
    Smtp(String),
    /// The sending task died (panic or runtime shutdown).
    #[error("mail task aborted")]
    Aborted,
}

impl MailError {
    /// The variant as a short word for logs.
    pub fn kind(&self) -> &'static str {
        match self {
            MailError::Address => "address",
            MailError::Build(_) => "build",
            MailError::Smtp(_) => "smtp",
            MailError::Aborted => "aborted",
        }
    }
}

/// An SMTP error as [`MailError::Smtp`]: its class and the reply code (e.g. `550`). The
/// error's own text is dropped: it quotes the server's reply, which often names the
/// recipient address.
fn smtp_error(err: &lettre::transport::smtp::Error) -> MailError {
    let class = if err.is_permanent() {
        "permanent"
    } else if err.is_transient() {
        "transient"
    } else if err.is_timeout() {
        "timeout"
    } else if err.is_tls() {
        "tls"
    } else if err.is_response() {
        "response"
    } else if err.is_client() {
        "client"
    } else {
        "connection"
    };
    match err.status() {
        Some(code) => MailError::Smtp(format!("{class}, code {code}")),
        None => MailError::Smtp(class.to_string()),
    }
}

/// Delivery status of one [`Mailer::send`]. Dropping it does not cancel the send.
#[derive(Debug)]
pub struct MailHandle {
    id: u64,
    task: JoinHandle<Result<(), MailError>>,
}

impl MailHandle {
    /// Sequence number of this mail in the process (matches the `mail_id` log field).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Waits until the mail was accepted by the SMTP server, or failed.
    pub async fn outcome(self) -> Result<(), MailError> {
        self.task.await.unwrap_or(Err(MailError::Aborted))
    }
}

/// SMTP mailer. Cheap to clone (shared transport and connection pool).
#[derive(Clone)]
pub struct Mailer {
    inner: Arc<Inner>,
}

struct Inner {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Result<Mailbox, MailError>,
    permits: Semaphore,
    next_id: AtomicU64,
    describe: String,
}

impl fmt::Debug for Mailer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Mailer").field(&self.inner.describe).finish()
    }
}

impl Mailer {
    /// Builds the transport from the configuration. Does not connect yet (the first send
    /// does), so it never fails at startup because the SMTP server is down. Create (and
    /// drop) it inside a Tokio runtime: the connection pool needs one.
    pub fn new(config: &MailConfig) -> Result<Self, MailError> {
        let builder = match config.encryption {
            MailEncryption::Ssl => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host),
            MailEncryption::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
            }
            MailEncryption::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                &config.host,
            )),
        }
        .map_err(|e| MailError::Smtp(e.to_string()))?;
        let mut builder = builder.port(config.port).timeout(Some(SMTP_TIMEOUT));
        if let (Some(user), Some(password)) = (&config.username, &config.password) {
            builder = builder.credentials(Credentials::new(user.clone(), password.clone()));
        }

        let from = format!("{} <{}>", config.from_name, config.from_address)
            .parse::<Mailbox>()
            .or_else(|_| config.from_address.parse::<Mailbox>())
            .map_err(|_| MailError::Address);
        if from.is_err() {
            tracing::error!("MAIL_FROM_ADDRESS is not a valid address; mails will fail");
        }

        Ok(Self {
            inner: Arc::new(Inner {
                transport: builder.build(),
                from,
                permits: Semaphore::new(MAX_CONCURRENT_SENDS),
                next_id: AtomicU64::new(1),
                describe: format!("{}:{} ({:?})", config.host, config.port, config.encryption),
            }),
        })
    }

    /// Queues `message` for delivery on a background task and returns immediately.
    /// Must be called inside a Tokio runtime (any server fn or handler is).
    pub fn send(&self, message: Message) -> MailHandle {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner.clone();
        let task = tokio::spawn(async move {
            let domain = recipient_domain(&message.to).to_string();
            let result = inner.deliver(message).await;
            match &result {
                Ok(()) => tracing::debug!(mail_id = id, to_domain = %domain, "mail sent"),
                Err(err) => {
                    tracing::warn!(mail_id = id, to_domain = %domain, kind = err.kind(), error = %err, "mail failed")
                }
            }
            result
        });
        MailHandle { id, task }
    }
}

impl Inner {
    async fn deliver(&self, message: Message) -> Result<(), MailError> {
        let email = self.build(message)?;
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| MailError::Aborted)?;
        self.transport
            .send(email)
            .await
            .map(|_| ())
            .map_err(|e| smtp_error(&e))
    }

    fn build(&self, message: Message) -> Result<lettre::Message, MailError> {
        let to = message
            .to
            .trim()
            .parse::<Mailbox>()
            .map_err(|_| MailError::Address)?;
        lettre::Message::builder()
            .from(self.from.clone()?)
            .to(to)
            .subject(message.subject)
            .multipart(MultiPart::alternative_plain_html(
                message.text,
                message.html,
            ))
            .map_err(|e| MailError::Build(e.to_string()))
    }
}

/// The part after `@` (for logs), or `"?"`.
fn recipient_domain(address: &str) -> &str {
    address
        .rsplit_once('@')
        .map(|(_, d)| d.trim_end_matches('>').trim())
        .unwrap_or("?")
}

/// Escapes text for HTML element content and quoted attribute values in mail templates:
/// `&`, `<`, `>`, `"`, `'`.
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + input.len() / 8);
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
