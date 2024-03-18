use chrono::{DateTime, Utc};

use super::{duration_to_secs, Counter};

#[derive(Clone, Debug)]
pub(super) struct DurationCounter {
    start: DateTime<Utc>,
}

impl Default for DurationCounter {
    fn default() -> Self {
        Self { start: Utc::now() }
    }
}

impl Counter for DurationCounter {
    fn count(&self) -> f64 {
        let duration = Utc::now() - self.start;
        duration_to_secs(duration)
    }
}
