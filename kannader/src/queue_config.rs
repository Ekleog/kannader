use std::time::Duration;

use async_trait::async_trait;
use tracing::{error, warn};

use smtp_queue::QueueId;

use crate::Meta;

pub struct QueueConfig(());

impl QueueConfig {
    pub fn new() -> QueueConfig {
        QueueConfig(())
    }
}

#[async_trait]
impl smtp_queue::Config<Meta, smtp_queue_fs::Error> for QueueConfig {
    async fn next_interval(&self, _s: smtp_queue::ScheduleInfo) -> Option<Duration> {
        // TODO: most definitely should try again
        // TODO: add bounce support to both transport and here
        None
    }

    async fn log_storage_error(&self, err: smtp_queue_fs::Error, id: Option<QueueId>) {
        error!(queue_id = ?id, error = ?anyhow::Error::new(err), "Storage error");
    }

    async fn log_found_inflight(&self, inflight: QueueId) {
        warn!(queue_id=?inflight, "Found inflight mail, waiting {:?} before sending", self.found_inflight_check_delay());
    }

    async fn log_found_pending_cleanup(&self, pcm: QueueId) {
        warn!(queue_id=?pcm, "Found mail pending cleanup");
    }

    async fn log_queued_mail_vanished(&self, id: QueueId) {
        error!(queue_id = ?id, "Queued mail vanished");
    }

    async fn log_inflight_mail_vanished(&self, id: QueueId) {
        error!(queue_id = ?id, "Inflight mail vanished");
    }

    async fn log_pending_cleanup_mail_vanished(&self, id: QueueId) {
        error!(queue_id = ?id, "Mail that was pending cleanup vanished");
    }

    async fn log_too_big_duration(&self, id: QueueId, too_big: Duration, new: Duration) {
        error!(queue_id = ?id, too_big = ?too_big, reset_to = ?new, "Ended up having too big a duration");
    }
}
