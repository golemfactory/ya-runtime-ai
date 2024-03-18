use tokio::runtime::Handle;
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

#[derive(Debug, Default)]
struct ResponseMonitors {
    counters: SharedCounters,
    // failsafe flag to count response on Drop if not counted already.
    counted: bool,
}

impl ResponseMonitor for ResponseMonitors {
    async fn on_response(mut self) {
        let counters = self.counters.write().await;
        if self.counted {
            return;
        };
        self.counted = true;
        ResponseMonitors::on_response(counters);
    }
}

impl ResponseMonitors {
    fn on_response(
        mut counters: tokio::sync::RwLockWriteGuard<'_, Vec<crate::counter::SupportedCounter>>,
    ) {
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
        self.counted = true;
        match Handle::try_current() {
            Ok(runtime) => {
                let counters = self.counters.clone();
                runtime.spawn(async move {
                    let counters = counters.write().await;
                    ResponseMonitors::on_response(counters);
                });
            }
            Err(_) => {
                let counters = self.counters.blocking_write();
                ResponseMonitors::on_response(counters);
            }
        };
    }
}
