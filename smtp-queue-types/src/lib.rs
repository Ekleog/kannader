use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct ScheduleInfo {
    pub at: DateTime<Utc>,
    pub last_attempt: Option<DateTime<Utc>>,
}

impl ScheduleInfo {
    pub fn last_interval(&self) -> Result<Option<Duration>, time::OutOfRangeError> {
        self.last_attempt
            .map(|last| (last - self.at).to_std())
            .transpose()
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct QueueId(pub Arc<String>);

impl QueueId {
    pub fn new<S: ToString>(s: S) -> QueueId {
        QueueId(Arc::new(s.to_string()))
    }
}
