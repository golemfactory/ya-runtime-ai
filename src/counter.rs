use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;
use ya_gsb_http_proxy::monitor::{RequestsMonitor, ResponseMonitor};


#[derive(Clone, Debug, Default)]
pub struct RequestsCounter {
    counter: Arc<RwLock<RequestsCounterCombined>>,
}

impl RequestsMonitor for RequestsCounter {
    async fn on_request(&mut self) -> impl ResponseMonitor {
        let mut counter = self.counter.write().await;
        counter.on_request();
        ResponseMonitorCombined { counted: false, counter: self.counter.clone() }
    }
}

#[derive(Clone, Debug, Default)]
struct ResponseMonitorCombined {
    // failsafe flag to count response on Drop if not counted already.
    counted: bool,
    counter: Arc<RwLock<RequestsCounterCombined>>,
}

impl ResponseMonitor for ResponseMonitorCombined {
    async fn on_response(mut self) {
        let mut counter = self.counter.write().await;
        if !self.counted {
            self.counted = true;
            counter.on_response().await;
        }
    }
}

impl Drop for ResponseMonitorCombined {
    fn drop(&mut self) {
        let dropped = std::mem::replace(self, ResponseMonitorCombined::default());
        if !dropped.counted {
            tokio::spawn(dropped.on_response());
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RequestsCounterCombined {
    duration_counter: RequestsDurationCounter,
}

impl RequestsCounterCombined {
    pub(crate) fn requests_duration(&self) -> f64 {
        self.duration_counter.count()
    }
    
    pub(crate) fn requests_count(&self) -> f64 {
        0.0
    }
}

impl RequestsCounterCombined {
    fn on_request(&mut self) {
        self.duration_counter.on_request();
        //TODO call request number counter
    }
    
    async fn on_response(&mut self) {
        self.duration_counter.on_response();
        //TODO call request number counter
    }
    
}

pub struct RequestCounter {}


#[derive(Clone, Copy, Debug)]
pub struct RequestsDurationCounter {
    duration: Duration,
    active_requests_count: u16,
    first_active_request_start_time: Option<DateTime<Utc>>,
}

impl RequestsDurationCounter {
    fn count(&self) -> f64 {
        let duration_so_far = self.duration + self.active_request_duration();
        Self::duration_to_secs(duration_so_far)
    }

    fn on_request(&mut self) {
        self.active_requests_count += 1;
        if self.first_active_request_start_time.is_none() {
            self.first_active_request_start_time = Some(Utc::now());
        }
    }

    fn on_response(&mut self) {
        self.active_requests_count -= 1;
        if self.active_requests_count == 0 {
            self.duration = self.duration + self.active_request_duration();
            self.first_active_request_start_time = None;
        }

    }

    fn active_request_duration(&self) -> Duration {
        if let Some(active_request_start_time) = self.first_active_request_start_time {
            let now = Utc::now();
            return now - active_request_start_time;
        }
        Duration::zero()
    }

    fn duration_to_secs(duration: Duration) -> f64 {
        duration.to_std().expect("Duration is bigger than 0").as_secs_f64()
    }
}

impl Default for RequestsDurationCounter {
    fn default() -> Self {
        let duration = Duration::zero();
        Self { duration, ..Default::default() }
    }
}
