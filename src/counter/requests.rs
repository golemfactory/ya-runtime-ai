use chrono::{DateTime, Utc};

use super::{Counter, RequestMonitoringCounter};

#[derive(Debug, Default)]
pub(super) struct RequestsCounter {
    count: u64,
}

impl Counter for RequestsCounter {
    fn count(&self) -> f64 {
        self.count as f64
    }
}

impl RequestMonitoringCounter for RequestsCounter {
    fn on_request(&mut self, _request_time: DateTime<Utc>) {
        self.count += 1;
    }

    fn on_response(&mut self, _response_time: DateTime<Utc>) {}
}
