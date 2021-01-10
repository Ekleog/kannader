use std::time::Duration;

use async_trait::async_trait;
use tracing::error;

use smtp_queue::QueueId;

use crate::{Meta, WASM_CONFIG};

pub struct QueueConfig(());

impl QueueConfig {
    pub fn new() -> QueueConfig {
        QueueConfig(())
    }
}

macro_rules! run_hook {
    ($fn:ident($($arg:expr),*) || $res:expr) => {
        WASM_CONFIG.with(|wasm_config| {
            match (wasm_config.queue_config.$fn)($($arg),*) {
                Ok(res) => res,
                Err(e) => {
                    error!(error = ?e, "Internal server in ‘queue_config_{}’", stringify!($fn));
                    $res
                }
            }
        })
    };
}

#[async_trait]
impl smtp_queue::Config<Meta, smtp_queue_fs::Error> for QueueConfig {
    async fn next_interval(&self, s: smtp_queue::ScheduleInfo) -> Option<Duration> {
        run_hook!(next_interval(s) || Some(std::time::Duration::from_secs(4 * 3600)))
    }

    async fn log_storage_error(&self, err: smtp_queue_fs::Error, id: Option<QueueId>) {
        run_hook!(log_storage_error(serde_error::Error::new(&err), id) || ())
    }

    async fn log_found_inflight(&self, inflight: QueueId) {
        run_hook!(log_found_inflight(inflight) || ())
    }

    async fn log_found_pending_cleanup(&self, pcm: QueueId) {
        run_hook!(log_found_pending_cleanup(pcm) || ())
    }

    async fn log_queued_mail_vanished(&self, id: QueueId) {
        run_hook!(log_queued_mail_vanished(id) || ())
    }

    async fn log_inflight_mail_vanished(&self, id: QueueId) {
        run_hook!(log_inflight_mail_vanished(id) || ())
    }

    async fn log_pending_cleanup_mail_vanished(&self, id: QueueId) {
        run_hook!(log_pending_cleanup_mail_vanished(id) || ())
    }

    async fn log_too_big_duration(&self, id: QueueId, too_big: Duration, new: Duration) {
        run_hook!(log_too_big_duration(id, too_big, new) || ())
    }

    fn found_inflight_check_delay(&self) -> Duration {
        run_hook!(found_inflight_check_delay() || Duration::from_secs(3600))
    }

    fn io_error_next_retry_delay(&self, d: Duration) -> Duration {
        run_hook!(
            io_error_next_retry_delay(d) || {
                if d < std::time::Duration::from_secs(30) {
                    std::time::Duration::from_secs(60)
                } else {
                    d.mul_f64(2.0)
                }
            }
        )
    }
}
