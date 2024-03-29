/*
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use ya_gsb_http_proxy::monitor::{RequestsMonitor, ResponseMonitor};

use super::SharedCounters;

#[derive(Clone, Debug)]
pub(super) struct RequestsMonitoringCounters {
    counters: SharedCounters,
    response_time_tx: UnboundedSender<DateTime<Utc>>,
}

impl RequestsMonitoringCounters {
    pub(super) fn start(counters: SharedCounters) -> Self {
        let (response_time_tx, response_time_rx) = tokio::sync::mpsc::unbounded_channel();
        Self::spawn_responses_receiver(response_time_rx, counters.clone());
        Self {
            counters,
            response_time_tx,
        }
    }

    fn spawn_responses_receiver(
        mut response_time_rx: UnboundedReceiver<DateTime<Utc>>,
        counters: SharedCounters,
    ) {
        tokio::spawn(async move {
            while let Some(response_time) = response_time_rx.recv().await {
                let mut counters = counters.write().await;
                for counter in &mut *counters {
                    if let Some(counter) = counter.request_monitoring_counter() {
                        counter.on_response(response_time);
                    }
                }
            }
        });
    }
}

impl RequestsMonitor for RequestsMonitoringCounters {
    async fn on_request(&mut self) -> impl ResponseMonitor {
        let mut counters = self.counters.write().await;
        let request_time = Utc::now();
        for counter in &mut *counters {
            if let Some(counter) = counter.request_monitoring_counter() {
                counter.on_request(request_time);
            }
        }
        let response_time_tx = self.response_time_tx.clone();
        ResponseMonitors::new(response_time_tx)
    }
}

#[derive(Debug)]
struct ResponseMonitors {
    response_time_tx: UnboundedSender<DateTime<Utc>>,
    // failsafe flag to count response on Drop if not counted already.
    counted: bool,
}

impl ResponseMonitors {
    fn new(response_time_tx: UnboundedSender<DateTime<Utc>>) -> Self {
        let counted = false;
        Self {
            response_time_tx,
            counted,
        }
    }
}

impl ResponseMonitor for ResponseMonitors {
    fn on_response(mut self) {
        ResponseMonitors::on_response(&mut self);
    }
}

impl ResponseMonitors {
    fn on_response(&mut self) {
        if self.counted {
            return;
        };
        self.counted = true;
        if let Err(error) = self.response_time_tx.send(Utc::now()) {
            log::error!("Faied to send response monitoring event. Err: {error}");
        }
    }
}

// Failsafe for not calling `on_response`.
impl Drop for ResponseMonitors {
    fn drop(&mut self) {
        self.on_response();
    }
}
*/
