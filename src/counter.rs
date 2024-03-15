use std::{str::FromStr, sync::Arc};

use anyhow::bail;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;
use ya_gsb_http_proxy::monitor::{RequestsMonitor, ResponseMonitor};

type SharedCounters = Arc<RwLock<Vec<SupportedCounter>>>;

#[derive(Clone, Debug, Default)]
pub struct Counters {
    counters: SharedCounters,
}

impl Counters {
    /// From list of Agreement counter names
    /// Fails on unsupported counter
    pub fn from_counters(counter_names: &Vec<String>) -> anyhow::Result<Self> {
        if counter_names.is_empty() {
            bail!("Agreement has no counters");
        }
        let mut counters = Vec::with_capacity(counter_names.len());
        for counter in counter_names {
            let counter = SupportedCounter::from_str(counter)?;
            counters.push(counter);
        }
        let counters = Arc::new(RwLock::new(counters));
        Ok(Self { counters })
    }

    /// Returns usage reported by counters in Agreement specified order.
    /// None if Agreement had no counter names.
    pub async fn current_usage(&self) -> Option<Vec<f64>> {
        let counters = self.counters.read().await;
        let current_usage = counters
            .iter()
            .map(Counter::count)
            .collect();
        Some(current_usage)
    }

    pub fn requests_monitor(&mut self) -> impl RequestsMonitor {
        let counters = self.counters.clone();
        RequestsCounters { counters }
    }
}

#[derive(Clone, Debug)]
enum SupportedCounter {
    Duration(DisabledCounter),
    RequestsDuration(RequestsDurationCounter),
    RequestsCount(DisabledCounter),
}

impl FromStr for SupportedCounter {

    type Err = anyhow::Error;

    fn from_str(counter: &str) -> anyhow::Result<Self, Self::Err> {
        match counter {
            "golem.usage.duration_sec"  => Ok(SupportedCounter::Duration(DisabledCounter::default())),
            "golem.usage.gpu-sec"       => Ok(SupportedCounter::RequestsDuration(Default::default())),
            "ai-runtime.requests"       => Ok(SupportedCounter::RequestsCount(DisabledCounter::default())),
            _                           => bail!("Unsupported counter: {}", counter),
        }
    }
}

impl SupportedCounter {
    fn request_monitoring_counter(&mut self) -> Option<&mut impl RequestMonitoringCounter> {
        match self {
            SupportedCounter::Duration(_) => None,
            SupportedCounter::RequestsDuration(counter) => Some(counter),
            SupportedCounter::RequestsCount(_) => None,
        }
    }
}

impl Counter for SupportedCounter {
    fn count(&self) -> f64 {
        match self {
            SupportedCounter::Duration(counter) => counter.count(),
            SupportedCounter::RequestsDuration(counter) => counter.count(),
            SupportedCounter::RequestsCount(counter) => counter.count(),
        }
    }
}

pub trait Counter {
    fn count(&self) -> f64;
}

#[derive(Clone, Debug, Default)]
struct DisabledCounter {}

impl Counter for DisabledCounter {
    fn count(&self) -> f64 {
        0.0
    }
}

pub trait RequestMonitoringCounter: Counter {
    fn on_request(&mut self);
    fn on_response(&mut self);
}

#[derive(Clone, Debug, Default)]
pub struct RequestsCounters {
    counters: SharedCounters,
}

impl RequestsMonitor for RequestsCounters {
    async fn on_request(&mut self) -> impl ResponseMonitor {
        let mut counters = self.counters.write().await;
        for counter in &mut *counters {
            if let Some(counter) = counter.request_monitoring_counter() {
                counter.on_request();
            }
        }
        let counters = self.counters.clone();
        ResponseMonitors { counters, ..Default::default() }
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
        let dropped = std::mem::take(self);
        if !dropped.counted {
            tokio::spawn(dropped.on_response());
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RequestsDurationCounter {
    duration: Duration,
    active_requests_count: u16,
    first_active_request_start_time: Option<DateTime<Utc>>,
}

impl RequestsDurationCounter {
    fn duration_to_secs(duration: Duration) -> f64 {
        duration.to_std().expect("Duration is bigger than 0").as_secs_f64()
    }
    
    fn active_request_duration(&self) -> Duration {
        if let Some(active_request_start_time) = self.first_active_request_start_time {
            let now = Utc::now();
            return now - active_request_start_time;
        }
        Duration::zero()
    }
}

impl Counter for RequestsDurationCounter {
    fn count(&self) -> f64 {
        let duration_so_far = self.duration + self.active_request_duration();
        Self::duration_to_secs(duration_so_far)
    }
}

impl RequestMonitoringCounter for RequestsDurationCounter {

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
}

impl Default for RequestsDurationCounter {
    fn default() -> Self {
        let duration = Duration::zero();
        Self { duration, active_requests_count: 0, first_active_request_start_time: None }
    }
}
