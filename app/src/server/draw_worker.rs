//! The draw mail worker (spec: draw, "Notification worker", D8).
//!
//! One Tokio task takes jobs from a channel **one at a time**, so the same rows are never
//! sent twice in parallel:
//! - [`DrawWorker::draw_result`] (`DrawResult { group_id }`): the group's unsent rows below
//!   the attempt limit, oldest first. Each row is re-read with all conditions **right before
//!   its mail is sent** (a reset or a group delete in the meantime drops it), sent to the
//!   giver's stored address in the giver's account language, then its delivery state is
//!   recorded. A failure is retried after `retry_delays[attempts - 1]`, up to
//!   [`MAX_ATTEMPTS`]; failures never undo the draw.
//! - [`DrawWorker::draw_reset`] (`DrawReset`): one reset mail per recipient, best effort,
//!   held in memory with the same retries (a restart loses them).
//! - [`DrawWorker::recover`]: at startup, a `DrawResult` for every drawn group with rows
//!   left to send (delivery is at-least-once).
//!
//! The mails go through a [`MailTransport`] (production: the [`Mailer`]; tests inject
//! failures and blocking). No DB connection is held while a mail is sent.
//!
//! Secrecy (S5): logs carry the group id, the giver's user id and attempt numbers only;
//! never names, addresses, mail texts or receiver ids.

use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use sqlx::{FromRow, MySqlPool};
use tokio::sync::{mpsc, watch};

use super::config::Config;
use super::draw_mail;
use super::i18n::mail_i18n;
use super::mail::{MailError, Mailer, Message};
use crate::error::AppError;

/// Delivery attempts per mail (the first one included).
pub const MAX_ATTEMPTS: u8 = 3;

/// Production waits before the second and the third attempt.
pub fn default_retry_delays() -> Vec<Duration> {
    vec![Duration::from_secs(60), Duration::from_secs(300)]
}

pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MailError>> + Send + 'a>>;

/// What the worker sends mails with. [`Mailer`] is the production implementation; tests
/// wrap a Mailpit-backed `Mailer` with failure/blocking hooks.
pub trait MailTransport: Send + Sync + 'static {
    fn send(&self, message: Message) -> SendFuture<'_>;
}

impl MailTransport for Mailer {
    fn send(&self, message: Message) -> SendFuture<'_> {
        let handle = Mailer::send(self, message);
        Box::pin(handle.outcome())
    }
}

/// One reset mail (held in memory only: D7, best effort).
#[derive(Clone)]
pub struct ResetRecipient {
    pub user_id: i32,
    /// The stored address.
    pub email: String,
    pub name: String,
    /// The stored `users.locale` (NULL or unknown: `APP_LOCALE`).
    pub locale: Option<String>,
}

/// The reset of one group's draw, for every former giver except the admin who reset it.
#[derive(Clone)]
pub struct DrawReset {
    pub group_id: i32,
    pub group_name: String,
    pub admin_name: String,
    pub recipients: Vec<ResetRecipient>,
}

enum Job {
    /// Send the group's unsent draw mails.
    DrawResult { group_id: i32 },
    /// The retry of one assignment row's draw mail.
    RetryRow { row_id: i32 },
    /// One reset mail; `attempt` is the number of this attempt (1-based).
    ResetMail {
        group_id: i32,
        group_name: Arc<str>,
        admin_name: Arc<str>,
        recipient: ResetRecipient,
        attempt: u8,
    },
}

impl Job {
    /// The job's kind and its id (group or row) for logs; nothing else.
    fn label(&self) -> (&'static str, i32) {
        match self {
            Job::DrawResult { group_id } => ("draw_result", *group_id),
            Job::RetryRow { row_id } => ("retry_row", *row_id),
            Job::ResetMail { group_id, .. } => ("reset_mail", *group_id),
        }
    }
}

struct Shared {
    pool: MySqlPool,
    config: Arc<Config>,
    transport: Arc<dyn MailTransport>,
    retry_delays: Vec<Duration>,
    jobs: mpsc::UnboundedSender<Job>,
    /// Jobs queued or running plus retries waiting for their delay.
    outstanding: watch::Sender<usize>,
    /// Rows whose retry is scheduled: a `DrawResult` of the group leaves them to the retry.
    retrying: Mutex<HashSet<i32>>,
}

/// Handle to the single worker task. Cheap to clone.
#[derive(Clone)]
pub struct DrawWorker {
    shared: Arc<Shared>,
}

impl fmt::Debug for DrawWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DrawWorker")
            .field("outstanding", &*self.shared.outstanding.borrow())
            .finish_non_exhaustive()
    }
}

impl DrawWorker {
    /// Spawns the worker (inside a Tokio runtime). `retry_delays[attempts - 1]` is the wait
    /// before attempt `attempts + 1` (production: 60 s, 300 s; tests: zero). The task ends
    /// when the last handle is dropped.
    pub fn new(
        pool: MySqlPool,
        config: Arc<Config>,
        transport: Arc<dyn MailTransport>,
        retry_delays: Vec<Duration>,
    ) -> Self {
        let (jobs, mut receiver) = mpsc::unbounded_channel();
        let (outstanding, _) = watch::channel(0usize);
        let shared = Arc::new(Shared {
            pool,
            config,
            transport,
            retry_delays,
            jobs,
            outstanding,
            retrying: Mutex::new(HashSet::new()),
        });
        let weak: Weak<Shared> = Arc::downgrade(&shared);
        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let Some(shared) = weak.upgrade() else { break };
                // Each job runs in its own task and is awaited here (still one at a time): a
                // panic ends only that job, never the worker.
                let (kind, id) = job.label();
                let runner = shared.clone();
                if tokio::spawn(async move { runner.run(job).await })
                    .await
                    .is_err()
                {
                    tracing::error!(job = kind, id, "draw worker: a job panicked; skipped");
                }
                shared.finish_one();
            }
        });
        DrawWorker { shared }
    }

    /// Queues the delivery of the group's unsent draw mails (after a draw or a resend).
    pub fn draw_result(&self, group_id: i32) {
        self.shared.enqueue(Job::DrawResult { group_id });
    }

    /// Queues the reset mails of one group.
    pub fn draw_reset(&self, reset: DrawReset) {
        let group_name: Arc<str> = reset.group_name.into();
        let admin_name: Arc<str> = reset.admin_name.into();
        for recipient in reset.recipients {
            self.shared.enqueue(Job::ResetMail {
                group_id: reset.group_id,
                group_name: group_name.clone(),
                admin_name: admin_name.clone(),
                recipient,
                attempt: 1,
            });
        }
    }

    /// Startup recovery: enqueues a `DrawResult` for every drawn group with unsent rows
    /// below the attempt limit.
    pub async fn recover(&self) -> Result<(), AppError> {
        let groups = sqlx::query_scalar::<_, i32>(
            "SELECT DISTINCT ga.group_id FROM gift_assignments ga \
             JOIN `groups` g ON g.id = ga.group_id \
             WHERE g.is_drawn = TRUE AND ga.notification_sent = FALSE \
               AND ga.notification_attempts < ? \
             ORDER BY ga.group_id",
        )
        .bind(MAX_ATTEMPTS)
        .fetch_all(&self.shared.pool)
        .await
        .map_err(|err| {
            tracing::error!(error = %super::db::redacted(&err), "draw worker: recovery query failed");
            AppError::internal("draw worker recovery")
        })?;
        if !groups.is_empty() {
            tracing::info!(
                groups = groups.len(),
                "draw worker: resuming unsent draw mails"
            );
        }
        for group_id in groups {
            self.draw_result(group_id);
        }
        Ok(())
    }

    /// Returns once the worker has nothing queued, in flight, or waiting for a retry.
    pub async fn wait_idle(&self) {
        let mut outstanding = self.shared.outstanding.subscribe();
        // The sender lives in `self.shared`, so the channel cannot close while we wait.
        let _ = outstanding.wait_for(|n| *n == 0).await;
    }
}

/// The columns of one assignment row the draw mail needs. Read only by the worker, only to
/// mail the giver their own receiver.
#[derive(FromRow)]
struct MailRow {
    group_id: i32,
    giver_id: i32,
    notification_attempts: u8,
    giver_email: String,
    giver_name: String,
    giver_locale: Option<String>,
    receiver_name: String,
    group_name: String,
}

impl Shared {
    fn enqueue(self: &Arc<Self>, job: Job) {
        self.outstanding.send_modify(|n| *n += 1);
        if self.jobs.send(job).is_err() {
            tracing::error!("draw worker: the worker task is gone; job dropped");
            self.finish_one();
        }
    }

    fn finish_one(&self) {
        self.outstanding.send_modify(|n| *n = n.saturating_sub(1));
    }

    /// The wait before attempt `attempts + 1`.
    fn delay_after(&self, attempts: u8) -> Duration {
        let index = usize::from(attempts.max(1) - 1);
        self.retry_delays
            .get(index)
            .or(self.retry_delays.last())
            .copied()
            .unwrap_or(Duration::ZERO)
    }

    /// Enqueues `job` after `delay`; counts as outstanding while it waits.
    fn schedule(self: &Arc<Self>, delay: Duration, job: Job) {
        self.outstanding.send_modify(|n| *n += 1);
        let shared = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            shared.enqueue(job);
            shared.finish_one();
        });
    }

    async fn run(self: &Arc<Self>, job: Job) {
        match job {
            Job::DrawResult { group_id } => self.send_group(group_id).await,
            Job::RetryRow { row_id } => {
                self.retrying_lock().remove(&row_id);
                self.send_row(row_id).await;
            }
            Job::ResetMail {
                group_id,
                group_name,
                admin_name,
                recipient,
                attempt,
            } => {
                self.send_reset(group_id, group_name, admin_name, recipient, attempt)
                    .await
            }
        }
    }

    fn retrying_lock(&self) -> std::sync::MutexGuard<'_, HashSet<i32>> {
        self.retrying
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn send_group(self: &Arc<Self>, group_id: i32) {
        let rows = sqlx::query_scalar::<_, i32>(
            "SELECT ga.id FROM gift_assignments ga JOIN `groups` g ON g.id = ga.group_id \
             WHERE ga.group_id = ? AND g.is_drawn = TRUE AND ga.notification_sent = FALSE \
               AND ga.notification_attempts < ? \
             ORDER BY ga.id",
        )
        .bind(group_id)
        .bind(MAX_ATTEMPTS)
        .fetch_all(&self.pool)
        .await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: could not read the rows to send");
                return;
            }
        };
        for row_id in rows {
            if self.retrying_lock().contains(&row_id) {
                continue;
            }
            self.send_row(row_id).await;
        }
    }

    /// Sends one row's draw mail if the row still needs it (re-checked right now).
    async fn send_row(self: &Arc<Self>, row_id: i32) {
        let row = sqlx::query_as::<_, MailRow>(
            "SELECT ga.group_id, ga.giver_id, ga.notification_attempts, \
                    giver.email AS giver_email, giver.name AS giver_name, \
                    giver.locale AS giver_locale, receiver.name AS receiver_name, \
                    g.name AS group_name \
             FROM gift_assignments ga \
             JOIN `groups` g ON g.id = ga.group_id \
             JOIN users giver ON giver.id = ga.giver_id \
             JOIN users receiver ON receiver.id = ga.receiver_id \
             WHERE ga.id = ? AND g.is_drawn = TRUE AND ga.notification_sent = FALSE \
               AND ga.notification_attempts < ?",
        )
        .bind(row_id)
        .bind(MAX_ATTEMPTS)
        .fetch_optional(&self.pool)
        .await;
        let row = match row {
            Ok(Some(row)) => row,
            // Sent, out of attempts, reset or deleted meanwhile: nothing to do.
            Ok(None) => return,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: could not read a row to send");
                return;
            }
        };
        let i18n = mail_i18n(row.giver_locale.as_deref(), &self.config);
        let message = draw_mail::draw_result(
            &i18n,
            &self.config,
            &row.giver_email,
            &row.giver_name,
            &row.receiver_name,
            row.group_id,
            &row.group_name,
        );
        let result = self.transport.send(message).await;
        let attempt = row.notification_attempts.saturating_add(1);
        let sent = result.is_ok();
        let update = sqlx::query(
            "UPDATE gift_assignments SET notification_sent = notification_sent OR ?, \
                    notification_attempts = LEAST(notification_attempts + 1, 255), \
                    notification_last_attempt_at = CURRENT_TIMESTAMP \
             WHERE id = ?",
        )
        .bind(sent)
        .bind(row_id)
        .execute(&self.pool)
        .await;
        if let Err(err) = update {
            tracing::error!(group_id = row.group_id, user_id = row.giver_id, error = %super::db::redacted(&err), "draw worker: could not record a delivery");
        }
        match result {
            Ok(()) => {
                tracing::debug!(
                    group_id = row.group_id,
                    user_id = row.giver_id,
                    attempt,
                    "draw mail sent"
                );
            }
            Err(err) => {
                tracing::warn!(
                    group_id = row.group_id,
                    user_id = row.giver_id,
                    attempt,
                    error = err.kind(),
                    "draw mail not delivered"
                );
                if attempt < MAX_ATTEMPTS {
                    self.retrying_lock().insert(row_id);
                    self.schedule(self.delay_after(attempt), Job::RetryRow { row_id });
                }
            }
        }
    }

    async fn send_reset(
        self: &Arc<Self>,
        group_id: i32,
        group_name: Arc<str>,
        admin_name: Arc<str>,
        recipient: ResetRecipient,
        attempt: u8,
    ) {
        let i18n = mail_i18n(recipient.locale.as_deref(), &self.config);
        let message = draw_mail::draw_reset(
            &i18n,
            &self.config,
            &recipient.email,
            &recipient.name,
            &admin_name,
            group_id,
            &group_name,
        );
        match self.transport.send(message).await {
            Ok(()) => tracing::debug!(
                group_id,
                user_id = recipient.user_id,
                attempt,
                "draw reset mail sent"
            ),
            Err(err) => {
                tracing::warn!(
                    group_id,
                    user_id = recipient.user_id,
                    attempt,
                    error = err.kind(),
                    "draw reset mail not delivered"
                );
                if attempt < MAX_ATTEMPTS {
                    let delay = self.delay_after(attempt);
                    self.schedule(
                        delay,
                        Job::ResetMail {
                            group_id,
                            group_name,
                            admin_name,
                            recipient,
                            attempt: attempt + 1,
                        },
                    );
                }
            }
        }
    }
}
