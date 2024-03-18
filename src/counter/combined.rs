use ya_gsb_http_proxy::monitor::{RequestsMonitor, ResponseMonitor};

use super::SharedCounters;

#[derive(Clone, Debug, Default)]
pub(super) struct RequestsMonitoringCounters {
    counters: SharedCounters,
}

impl RequestsMonitoringCounters {
    pub(super) fn new(counters: SharedCounters) -> Self {
        Self { counters }
    }
}

impl RequestsMonitor for RequestsMonitoringCounters {
    async fn on_request(&mut self) -> impl ResponseMonitor {
        let mut counters = self.counters.write().await;
        for counter in &mut *counters {
            if let Some(counter) = counter.request_monitoring_counter() {
                counter.on_request();
            }
        }
        let counters = self.counters.clone();
        ResponseMonitors {
            counters,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ResponseMonitors {
    counters: SharedCounters,
    // failsafe flag to count response on Drop if not counted already.
    counted: bool,
}

impl ResponseMonitor for ResponseMonitors {
    async fn on_response(mut self) {
        let mut counters = self.counters.write().await;
        if self.counted {
            return;
        };
        self.counted = true;
        for counter in &mut *counters {
            if let Some(counter) = counter.request_monitoring_counter() {
                counter.on_response();
            }
        }
    }
}

// Failsafe for not calling `on_response`.
impl Drop for ResponseMonitors {
    fn drop(&mut self) {
        if self.counted {
            return;
        }
        let dropped = std::mem::replace(self, Default::default());
        if !dropped.counted {
            tokio::spawn(dropped.on_response());
        }
    }
}
