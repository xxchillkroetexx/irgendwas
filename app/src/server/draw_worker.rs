//! Durable, at-least-once delivery for draw and reset notifications.
//!
//! Assignment rows are the result outbox; reset rows are inserted in the same
//! transaction as the reset. The channel is only a wake-up hint: periodic recovery
//! finds committed work even when a request or worker exits before enqueuing it.
//! Per-row leases prevent concurrent workers or application replicas from sending
//! the same row at once under normal operation. SMTP success followed by a crash or
//! failed database acknowledgement can still cause a duplicate after lease expiry.
//! No database connection is held while sending mail, and logs never contain names,
//! addresses, mail bodies or giver/receiver pairings.

use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use sqlx::{FromRow, MySqlPool};
use tokio::sync::{Semaphore, mpsc, watch};

use super::config::Config;
use super::draw_mail;
use super::i18n::mail_i18n;
use super::mail::{MailError, Mailer, Message};
use crate::error::AppError;

pub const MAX_ATTEMPTS: u8 = 3;
const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);
// Keep the lease longer than the entire cancellable delivery, not merely one SMTP command.
const SEND_TIMEOUT: Duration = Duration::from_secs(180);
const LEASE_SECONDS: i64 = 240;
const MAX_CONCURRENT_JOBS: usize = 4;
const BUSY_RETRY_DELAY: Duration = Duration::from_secs(1);

pub fn default_retry_delays() -> Vec<Duration> {
    vec![Duration::from_secs(60), Duration::from_secs(300)]
}

pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MailError>> + Send + 'a>>;

pub trait MailTransport: Send + Sync + 'static {
    fn send(&self, message: Message) -> SendFuture<'_>;
}

impl MailTransport for Mailer {
    fn send(&self, message: Message) -> SendFuture<'_> {
        Box::pin(self.send_direct(message))
    }
}

enum Job {
    DrawResult { group_id: i32 },
    RetryGroup { group_id: i32 },
    RetryRow { row_id: i32 },
    ResetGroup { group_id: i32 },
    RetryResetGroup { group_id: i32 },
    RetryReset { row_id: i32 },
}

impl Job {
    fn label(&self) -> (&'static str, i32) {
        match self {
            Self::DrawResult { group_id } => ("draw_result", *group_id),
            Self::RetryGroup { group_id } => ("retry_group", *group_id),
            Self::RetryRow { row_id } => ("retry_row", *row_id),
            Self::ResetGroup { group_id } => ("reset_group", *group_id),
            Self::RetryResetGroup { group_id } => ("retry_reset_group", *group_id),
            Self::RetryReset { row_id } => ("retry_reset", *row_id),
        }
    }
}

struct Shared {
    pool: MySqlPool,
    config: Arc<Config>,
    transport: Arc<dyn MailTransport>,
    retry_delays: Vec<Duration>,
    jobs: mpsc::UnboundedSender<Job>,
    outstanding: watch::Sender<usize>,
    retrying: Mutex<HashSet<i32>>,
    retrying_reset: Mutex<HashSet<i32>>,
    // Prevent periodic scans from piling up group hints behind a slow transport.
    queued_draw_groups: Mutex<HashSet<i32>>,
    queued_reset_groups: Mutex<HashSet<i32>>,
    deferred_draw_groups: Mutex<HashSet<i32>>,
    deferred_reset_groups: Mutex<HashSet<i32>>,
}

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
    pub fn new(
        pool: MySqlPool,
        config: Arc<Config>,
        transport: Arc<dyn MailTransport>,
        retry_delays: Vec<Duration>,
    ) -> Self {
        Self::with_interval(pool, config, transport, retry_delays, RECONCILE_INTERVAL)
    }

    #[cfg(feature = "test-support")]
    pub fn with_reconcile_interval(
        pool: MySqlPool,
        config: Arc<Config>,
        transport: Arc<dyn MailTransport>,
        retry_delays: Vec<Duration>,
        interval: Duration,
    ) -> Self {
        Self::with_interval(pool, config, transport, retry_delays, interval)
    }

    fn with_interval(
        pool: MySqlPool,
        config: Arc<Config>,
        transport: Arc<dyn MailTransport>,
        retry_delays: Vec<Duration>,
        interval: Duration,
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
            retrying_reset: Mutex::new(HashSet::new()),
            queued_draw_groups: Mutex::new(HashSet::new()),
            queued_reset_groups: Mutex::new(HashSet::new()),
            deferred_draw_groups: Mutex::new(HashSet::new()),
            deferred_reset_groups: Mutex::new(HashSet::new()),
        });
        let weak: Weak<Shared> = Arc::downgrade(&shared);
        tokio::spawn(async move {
            let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS));
            while let Some(job) = receiver.recv().await {
                let Some(shared) = weak.upgrade() else { break };
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    break;
                };
                let (kind, id) = job.label();
                let runner = shared.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if tokio::spawn(async move { runner.run(job).await })
                        .await
                        .is_err()
                    {
                        tracing::error!(
                            job = kind,
                            id,
                            "draw worker: job panicked; recovery will retry"
                        );
                    }
                    match kind {
                        "draw_result" => {
                            lock(&shared.queued_draw_groups).remove(&id);
                        }
                        "reset_group" => {
                            lock(&shared.queued_reset_groups).remove(&id);
                        }
                        _ => {}
                    }
                    shared.finish_one();
                });
            }
        });
        let weak = Arc::downgrade(&shared);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(shared) = weak.upgrade() else { break };
                if let Err(err) = shared.recover().await {
                    tracing::warn!(?err, "draw worker: periodic recovery failed");
                }
            }
        });
        Self { shared }
    }

    pub fn draw_result(&self, group_id: i32) {
        self.shared.enqueue_draw_group(group_id);
    }

    pub fn draw_reset(&self, group_id: i32) {
        self.shared.enqueue_reset_group(group_id);
    }

    pub async fn recover(&self) -> Result<(), AppError> {
        self.shared.recover().await
    }

    pub async fn wait_idle(&self) {
        let mut outstanding = self.shared.outstanding.subscribe();
        let _ = outstanding.wait_for(|n| *n == 0).await;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

#[derive(FromRow)]
struct ResetRow {
    group_id: i32,
    recipient_id: i32,
    attempts: u8,
    recipient_email: String,
    recipient_name: String,
    recipient_locale: Option<String>,
    group_name: String,
    admin_name: String,
}

impl Shared {
    fn enqueue(self: &Arc<Self>, job: Job) {
        self.outstanding.send_modify(|n| *n += 1);
        if self.jobs.send(job).is_err() {
            tracing::error!("draw worker: wake-up dropped; recovery will retry");
            self.finish_one();
        }
    }

    fn enqueue_draw_group(self: &Arc<Self>, group_id: i32) {
        if lock(&self.queued_draw_groups).insert(group_id) {
            self.enqueue(Job::DrawResult { group_id });
        }
    }

    fn enqueue_reset_group(self: &Arc<Self>, group_id: i32) {
        if lock(&self.queued_reset_groups).insert(group_id) {
            self.enqueue(Job::ResetGroup { group_id });
        }
    }

    fn defer_draw_group(self: &Arc<Self>, group_id: i32) {
        if lock(&self.deferred_draw_groups).insert(group_id) {
            self.schedule(BUSY_RETRY_DELAY, Job::RetryGroup { group_id });
        }
    }

    fn defer_reset_group(self: &Arc<Self>, group_id: i32) {
        if lock(&self.deferred_reset_groups).insert(group_id) {
            self.schedule(BUSY_RETRY_DELAY, Job::RetryResetGroup { group_id });
        }
    }

    fn finish_one(&self) {
        self.outstanding.send_modify(|n| *n = n.saturating_sub(1));
    }

    fn delay_after(&self, attempts: u8) -> Duration {
        let index = usize::from(attempts.max(1) - 1);
        self.retry_delays
            .get(index)
            .or(self.retry_delays.last())
            .copied()
            .unwrap_or(Duration::ZERO)
    }

    fn schedule(self: &Arc<Self>, delay: Duration, job: Job) {
        self.outstanding.send_modify(|n| *n += 1);
        let shared = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            shared.enqueue(job);
            shared.finish_one();
        });
    }

    async fn recover(self: &Arc<Self>) -> Result<(), AppError> {
        // A user who has left or been removed must not receive a still-pending
        // notice. A send already in flight cannot be recalled.
        sqlx::query(
            "DELETE n FROM draw_reset_notifications n LEFT JOIN group_members gm \
             ON gm.group_id = n.group_id AND gm.user_id = n.recipient_id \
             WHERE gm.user_id IS NULL",
        ).execute(&self.pool).await.map_err(|err| {
            tracing::error!(error = %super::db::redacted(&err), "draw worker: stale reset cleanup failed");
            AppError::internal("draw reset cleanup")
        })?;
        let groups = sqlx::query_scalar::<_, i32>(
            "SELECT DISTINCT ga.group_id FROM gift_assignments ga \
             JOIN `groups` g ON g.id = ga.group_id \
             WHERE g.is_drawn = TRUE AND ga.notification_sent = FALSE \
             AND ga.notification_attempts < ? \
             AND (ga.notification_claim_until IS NULL OR ga.notification_claim_until <= CURRENT_TIMESTAMP(6)) \
             AND (ga.notification_next_attempt_at IS NULL OR ga.notification_next_attempt_at <= CURRENT_TIMESTAMP(6)) \
             ORDER BY ga.group_id",
        ).bind(MAX_ATTEMPTS).fetch_all(&self.pool).await.map_err(|err| {
            tracing::error!(error = %super::db::redacted(&err), "draw worker: recovery query failed");
            AppError::internal("draw worker recovery")
        })?;
        for group in groups {
            self.enqueue_draw_group(group);
        }
        let groups = sqlx::query_scalar::<_, i32>(
            "SELECT DISTINCT n.group_id FROM draw_reset_notifications n JOIN `groups` g ON g.id = n.group_id \
             WHERE g.is_drawn = FALSE AND n.generation = g.draw_generation AND n.attempts < ? \
             AND (n.claim_until IS NULL OR n.claim_until <= CURRENT_TIMESTAMP(6)) \
             AND (n.next_attempt_at IS NULL OR n.next_attempt_at <= CURRENT_TIMESTAMP(6)) ORDER BY n.group_id",
        ).bind(MAX_ATTEMPTS).fetch_all(&self.pool).await.map_err(|err| {
            tracing::error!(error = %super::db::redacted(&err), "draw worker: reset recovery query failed");
            AppError::internal("draw reset recovery")
        })?;
        for group in groups {
            self.enqueue_reset_group(group);
        }
        Ok(())
    }

    async fn run(self: &Arc<Self>, job: Job) {
        match job {
            Job::DrawResult { group_id } => self.send_group(group_id).await,
            Job::RetryGroup { group_id } => {
                lock(&self.deferred_draw_groups).remove(&group_id);
                let queued = lock(&self.queued_draw_groups).contains(&group_id);
                if queued {
                    self.defer_draw_group(group_id);
                } else {
                    self.enqueue_draw_group(group_id);
                }
            }
            Job::RetryRow { row_id } => {
                lock(&self.retrying).remove(&row_id);
                if let Some(group_id) = self.send_row(row_id).await {
                    self.defer_draw_group(group_id);
                }
            }
            Job::ResetGroup { group_id } => self.send_reset_group(group_id).await,
            Job::RetryResetGroup { group_id } => {
                lock(&self.deferred_reset_groups).remove(&group_id);
                let queued = lock(&self.queued_reset_groups).contains(&group_id);
                if queued {
                    self.defer_reset_group(group_id);
                } else {
                    self.enqueue_reset_group(group_id);
                }
            }
            Job::RetryReset { row_id } => {
                lock(&self.retrying_reset).remove(&row_id);
                if let Some(group_id) = self.send_reset(row_id).await {
                    self.defer_reset_group(group_id);
                }
            }
        }
    }

    async fn send_group(self: &Arc<Self>, group_id: i32) {
        let rows = sqlx::query_scalar::<_, i32>(
            "SELECT ga.id FROM gift_assignments ga JOIN `groups` g ON g.id = ga.group_id \
             WHERE ga.group_id = ? AND g.is_drawn = TRUE AND ga.notification_sent = FALSE \
             AND ga.notification_attempts < ? ORDER BY ga.id",
        )
        .bind(group_id)
        .bind(MAX_ATTEMPTS)
        .fetch_all(&self.pool)
        .await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: could not read rows; recovery will retry");
                return;
            }
        };
        for row_id in rows {
            let retrying = lock(&self.retrying).contains(&row_id);
            if !retrying && self.send_row(row_id).await.is_some() {
                // Another replica owns this group's SMTP lease. Retry after this
                // group hint has left the deduplication set.
                self.defer_draw_group(group_id);
                break;
            }
        }
    }

    /// Returns the group only when another worker currently owns its lease.
    async fn send_row(self: &Arc<Self>, row_id: i32) -> Option<i32> {
        let group_id = match sqlx::query_scalar::<_, i32>(
            "SELECT group_id FROM gift_assignments WHERE id = ?",
        )
        .bind(row_id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(Some(group_id)) => group_id,
            Ok(None) => return None,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: cannot find group; recovery will retry");
                return None;
            }
        };
        let Ok(claim) = super::token::generate() else {
            tracing::error!("draw worker: could not generate claim");
            return None;
        };
        // Claim/renew/release are technical writes: preserve the current value of
        // groups.updated_at (which otherwise changes via ON UPDATE CURRENT_TIMESTAMP).
        let group_claim = sqlx::query(
            "UPDATE `groups` SET draw_mail_claim = ?, draw_mail_claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)), updated_at = updated_at \
             WHERE id = ? AND is_drawn = TRUE AND (draw_mail_claim_until IS NULL OR draw_mail_claim_until <= CURRENT_TIMESTAMP(6))",
        ).bind(&claim).bind(LEASE_SECONDS).bind(group_id).execute(&self.pool).await;
        match group_claim {
            Ok(result) if result.rows_affected() == 1 => {}
            Ok(_) => {
                // A reset or deletion is not contention; do not retry forever.
                return match sqlx::query_scalar::<_, bool>(
                    "SELECT is_drawn FROM `groups` WHERE id = ?",
                )
                .bind(group_id)
                .fetch_optional(&self.pool)
                .await
                {
                    Ok(Some(true)) => Some(group_id),
                    Ok(Some(false)) | Ok(None) => None,
                    Err(err) => {
                        tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: cannot read group status");
                        None
                    }
                };
            }
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: group claim failed; recovery will retry");
                return None;
            }
        }
        self.send_claimed_row(row_id, group_id, &claim).await;
        if let Err(err) = sqlx::query("UPDATE `groups` SET draw_mail_claim = NULL, draw_mail_claim_until = NULL, updated_at = updated_at WHERE id = ? AND draw_mail_claim = ?")
            .bind(group_id).bind(&claim).execute(&self.pool).await {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: group release failed; lease will expire");
            }
        None
    }

    async fn send_claimed_row(self: &Arc<Self>, row_id: i32, group_id: i32, claim: &str) {
        let claimed = sqlx::query(
            "UPDATE gift_assignments ga JOIN `groups` g ON g.id = ga.group_id \
             SET ga.notification_claim = ?, ga.notification_claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)) \
             WHERE ga.id = ? AND g.is_drawn = TRUE AND ga.notification_sent = FALSE AND ga.notification_attempts < ? \
             AND (ga.notification_claim_until IS NULL OR ga.notification_claim_until <= CURRENT_TIMESTAMP(6)) \
             AND (ga.notification_next_attempt_at IS NULL OR ga.notification_next_attempt_at <= CURRENT_TIMESTAMP(6))",
        ).bind(claim).bind(LEASE_SECONDS).bind(row_id).bind(MAX_ATTEMPTS).execute(&self.pool).await;
        match claimed {
            Ok(result) if result.rows_affected() == 1 => {}
            Ok(_) => return,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: claim failed; recovery will retry");
                return;
            }
        }
        let row = sqlx::query_as::<_, MailRow>(
            "SELECT ga.group_id, ga.giver_id, ga.notification_attempts, giver.email AS giver_email, \
                    giver.name AS giver_name, giver.locale AS giver_locale, receiver.name AS receiver_name, g.name AS group_name \
             FROM gift_assignments ga JOIN `groups` g ON g.id = ga.group_id \
             JOIN users giver ON giver.id = ga.giver_id JOIN users receiver ON receiver.id = ga.receiver_id \
             WHERE ga.id = ? AND ga.notification_claim = ? AND g.is_drawn = TRUE AND ga.notification_sent = FALSE",
        ).bind(row_id).bind(claim).fetch_optional(&self.pool).await;
        let row = match row {
            Ok(Some(row)) => row,
            Ok(None) => return,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: could not read claimed row; recovery will retry");
                return;
            }
        };
        // Verify that neither lease was superseded while a database query stalled.
        // Both are extended immediately before the bounded, cancellable SMTP call.
        let renewed = sqlx::query(
            "UPDATE `groups` g JOIN gift_assignments ga ON ga.group_id = g.id \
             SET g.draw_mail_claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)), g.updated_at = g.updated_at, \
                 ga.notification_claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)) \
             WHERE g.id = ? AND ga.id = ? AND g.is_drawn = TRUE AND g.draw_mail_claim = ? AND ga.notification_claim = ?",
        ).bind(LEASE_SECONDS).bind(LEASE_SECONDS).bind(group_id).bind(row_id).bind(claim).bind(claim).execute(&self.pool).await;
        match renewed {
            Ok(result) if result.rows_affected() > 0 => {}
            Ok(_) => return,
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: could not renew claims");
                return;
            }
        }
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
        let result = tokio::time::timeout(SEND_TIMEOUT, self.transport.send(message))
            .await
            .unwrap_or_else(|_| Err(MailError::Smtp("delivery timeout".into())));
        let attempt = row.notification_attempts.saturating_add(1);
        let delay = self.delay_after(attempt);
        let micros = i64::try_from(delay.as_micros()).unwrap_or(i64::MAX);
        let updated = sqlx::query(
            "UPDATE gift_assignments SET notification_sent = notification_sent OR ?, \
             notification_attempts = LEAST(notification_attempts + 1, 255), notification_last_attempt_at = CURRENT_TIMESTAMP, \
             notification_claim = NULL, notification_claim_until = NULL, \
             notification_next_attempt_at = TIMESTAMPADD(MICROSECOND, ?, CURRENT_TIMESTAMP(6)) \
             WHERE id = ? AND notification_claim = ?",
        ).bind(result.is_ok()).bind(micros).bind(row_id).bind(claim).execute(&self.pool).await;
        let recorded = match updated {
            Ok(result) => result.rows_affected() == 1,
            Err(err) => {
                tracing::error!(group_id = row.group_id, user_id = row.giver_id, error = %super::db::redacted(&err), "draw worker: acknowledgement failed; recovery will retry");
                false
            }
        };
        match result {
            Ok(()) => tracing::debug!(
                group_id = row.group_id,
                user_id = row.giver_id,
                attempt,
                "draw mail sent"
            ),
            Err(err) => {
                tracing::warn!(
                    group_id = row.group_id,
                    user_id = row.giver_id,
                    attempt,
                    error = err.kind(),
                    "draw mail not delivered"
                );
                if recorded && attempt < MAX_ATTEMPTS {
                    lock(&self.retrying).insert(row_id);
                    self.schedule(delay, Job::RetryRow { row_id });
                }
            }
        }
    }

    async fn send_reset_group(self: &Arc<Self>, group_id: i32) {
        let rows = sqlx::query_scalar::<_, i32>(
            "SELECT n.id FROM draw_reset_notifications n JOIN `groups` g ON g.id = n.group_id \
             WHERE n.group_id = ? AND g.is_drawn = FALSE AND n.generation = g.draw_generation AND n.attempts < ? ORDER BY n.id",
        ).bind(group_id).bind(MAX_ATTEMPTS).fetch_all(&self.pool).await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: could not read reset rows; recovery will retry");
                return;
            }
        };
        for row_id in rows {
            let retrying = lock(&self.retrying_reset).contains(&row_id);
            if !retrying && self.send_reset(row_id).await.is_some() {
                self.defer_reset_group(group_id);
                break;
            }
        }
    }

    async fn send_reset(self: &Arc<Self>, row_id: i32) -> Option<i32> {
        let target = sqlx::query_as::<_, (i32, u64)>(
            "SELECT n.group_id, n.generation FROM draw_reset_notifications n \
             JOIN group_members gm ON gm.group_id = n.group_id AND gm.user_id = n.recipient_id WHERE n.id = ?",
        ).bind(row_id).fetch_optional(&self.pool).await;
        let (group_id, generation) = match target {
            Ok(Some(target)) => target,
            Ok(None) => return None,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: cannot find reset group");
                return None;
            }
        };
        let Ok(claim) = super::token::generate() else {
            tracing::error!("draw worker: could not generate reset claim");
            return None;
        };
        let group_claim = sqlx::query(
            "UPDATE `groups` SET draw_mail_claim = ?, draw_mail_claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)), updated_at = updated_at \
             WHERE id = ? AND is_drawn = FALSE AND draw_generation = ? \
             AND (draw_mail_claim_until IS NULL OR draw_mail_claim_until <= CURRENT_TIMESTAMP(6))",
        ).bind(&claim).bind(LEASE_SECONDS).bind(group_id).bind(generation).execute(&self.pool).await;
        match group_claim {
            Ok(result) if result.rows_affected() == 1 => {}
            Ok(_) => {
                return match sqlx::query_as::<_, (bool, u64)>(
                    "SELECT is_drawn, draw_generation FROM `groups` WHERE id = ?",
                )
                .bind(group_id)
                .fetch_optional(&self.pool)
                .await
                {
                    Ok(Some((false, current))) if current == generation => Some(group_id),
                    Ok(_) => None,
                    Err(err) => {
                        tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: cannot read reset group status");
                        None
                    }
                };
            }
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: reset group claim failed");
                return None;
            }
        }
        self.send_claimed_reset(row_id, group_id, &claim).await;
        if let Err(err) = sqlx::query("UPDATE `groups` SET draw_mail_claim = NULL, draw_mail_claim_until = NULL, updated_at = updated_at WHERE id = ? AND draw_mail_claim = ?")
            .bind(group_id).bind(&claim).execute(&self.pool).await {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: reset group release failed; lease will expire");
            }
        None
    }

    async fn send_claimed_reset(self: &Arc<Self>, row_id: i32, group_id: i32, claim: &str) {
        let claimed = sqlx::query(
            "UPDATE draw_reset_notifications n JOIN `groups` g ON g.id = n.group_id \
             JOIN group_members gm ON gm.group_id = n.group_id AND gm.user_id = n.recipient_id \
             SET n.claim = ?, n.claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)) \
             WHERE n.id = ? AND g.is_drawn = FALSE AND n.generation = g.draw_generation AND n.attempts < ? \
             AND (n.claim_until IS NULL OR n.claim_until <= CURRENT_TIMESTAMP(6)) \
             AND (n.next_attempt_at IS NULL OR n.next_attempt_at <= CURRENT_TIMESTAMP(6))",
        ).bind(claim).bind(LEASE_SECONDS).bind(row_id).bind(MAX_ATTEMPTS).execute(&self.pool).await;
        match claimed {
            Ok(result) if result.rows_affected() == 1 => {}
            Ok(_) => return,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: reset claim failed; recovery will retry");
                return;
            }
        }
        let row = sqlx::query_as::<_, ResetRow>(
            "SELECT n.group_id, n.recipient_id, n.attempts, u.email AS recipient_email, u.name AS recipient_name, \
                    u.locale AS recipient_locale, n.group_name, n.admin_name \
             FROM draw_reset_notifications n JOIN `groups` g ON g.id = n.group_id JOIN users u ON u.id = n.recipient_id \
             JOIN group_members gm ON gm.group_id = n.group_id AND gm.user_id = n.recipient_id \
             WHERE n.id = ? AND n.claim = ? AND g.is_drawn = FALSE AND n.generation = g.draw_generation",
        ).bind(row_id).bind(claim).fetch_optional(&self.pool).await;
        let row = match row {
            Ok(Some(row)) => row,
            Ok(None) => return,
            Err(err) => {
                tracing::error!(error = %super::db::redacted(&err), "draw worker: could not read reset claim; recovery will retry");
                return;
            }
        };
        let renewed = sqlx::query(
            "UPDATE `groups` g JOIN draw_reset_notifications n ON n.group_id = g.id \
             JOIN group_members gm ON gm.group_id = n.group_id AND gm.user_id = n.recipient_id \
             SET g.draw_mail_claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)), g.updated_at = g.updated_at, \
                 n.claim_until = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6)) \
             WHERE g.id = ? AND n.id = ? AND g.is_drawn = FALSE AND n.generation = g.draw_generation \
             AND g.draw_mail_claim = ? AND n.claim = ?",
        ).bind(LEASE_SECONDS).bind(LEASE_SECONDS).bind(group_id).bind(row_id).bind(claim).bind(claim).execute(&self.pool).await;
        match renewed {
            Ok(result) if result.rows_affected() > 0 => {}
            Ok(_) => return,
            Err(err) => {
                tracing::error!(group_id, error = %super::db::redacted(&err), "draw worker: could not renew reset claims");
                return;
            }
        }
        let i18n = mail_i18n(row.recipient_locale.as_deref(), &self.config);
        let message = draw_mail::draw_reset(
            &i18n,
            &self.config,
            &row.recipient_email,
            &row.recipient_name,
            &row.admin_name,
            row.group_id,
            &row.group_name,
        );
        let result = tokio::time::timeout(SEND_TIMEOUT, self.transport.send(message))
            .await
            .unwrap_or_else(|_| Err(MailError::Smtp("delivery timeout".into())));
        let attempt = row.attempts.saturating_add(1);
        if result.is_ok() || attempt >= MAX_ATTEMPTS {
            if let Err(err) =
                sqlx::query("DELETE FROM draw_reset_notifications WHERE id = ? AND claim = ?")
                    .bind(row_id)
                    .bind(claim)
                    .execute(&self.pool)
                    .await
            {
                tracing::error!(group_id = row.group_id, user_id = row.recipient_id, error = %super::db::redacted(&err), "draw worker: reset acknowledgement failed; recovery will retry");
            }
        } else {
            let delay = self.delay_after(attempt);
            let micros = i64::try_from(delay.as_micros()).unwrap_or(i64::MAX);
            let updated = sqlx::query(
                "UPDATE draw_reset_notifications SET attempts = attempts + 1, claim = NULL, claim_until = NULL, \
                 next_attempt_at = TIMESTAMPADD(MICROSECOND, ?, CURRENT_TIMESTAMP(6)) WHERE id = ? AND claim = ?",
            ).bind(micros).bind(row_id).bind(claim).execute(&self.pool).await;
            match updated {
                Ok(result) if result.rows_affected() == 1 => {
                    lock(&self.retrying_reset).insert(row_id);
                    self.schedule(delay, Job::RetryReset { row_id });
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(group_id = row.group_id, user_id = row.recipient_id, error = %super::db::redacted(&err), "draw worker: reset retry save failed")
                }
            }
        }
        match result {
            Ok(()) => tracing::debug!(
                group_id = row.group_id,
                user_id = row.recipient_id,
                attempt,
                "draw reset mail sent"
            ),
            Err(err) => tracing::warn!(
                group_id = row.group_id,
                user_id = row.recipient_id,
                attempt,
                error = err.kind(),
                "draw reset mail not delivered"
            ),
        }
    }
}
