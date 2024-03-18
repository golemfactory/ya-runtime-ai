mod combined;
mod duration;
mod requests;
mod requests_duration;

use std::{str::FromStr, sync::Arc};

use anyhow::bail;
use chrono::Duration;
use tokio::sync::RwLock;
use ya_gsb_http_proxy::monitor::RequestsMonitor;

use self::{
    combined::RequestsMonitoringCounters, duration::DurationCounter, requests::RequestsCounter,
    requests_duration::RequestsDurationCounter,
};

type SharedCounters = Arc<RwLock<Vec<SupportedCounter>>>;

#[derive(Clone, Debug, Default)]
pub struct Counters {
    counters: SharedCounters,
}

impl Counters {
    /// From list of Agreement counter names
    /// Fails on unsupported counter
    pub fn from_counters(counter_names: &Vec<String>) -> anyhow::Result<Self> {
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
        let current_usage = counters.iter().map(Counter::count).collect();
        Some(current_usage)
    }

    pub fn requests_monitor(&self) -> impl RequestsMonitor {
        let counters = self.counters.clone();
        RequestsMonitoringCounters::new(counters)
    }
}

#[derive(Clone, Debug)]
enum SupportedCounter {
    Duration(DurationCounter),
    RequestsDuration(RequestsDurationCounter),
    RequestsCount(RequestsCounter),
}

impl FromStr for SupportedCounter {
    type Err = anyhow::Error;

    fn from_str(counter: &str) -> anyhow::Result<Self, Self::Err> {
        let counter = match counter {
            "golem.usage.duration_sec" => SupportedCounter::Duration(Default::default()),
            "golem.usage.gpu-sec" => SupportedCounter::RequestsDuration(Default::default()),
            "ai-runtime.requests" => SupportedCounter::RequestsCount(Default::default()),
            _ => bail!("Unsupported counter: {}", counter),
        };
        Ok(counter)
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
        let counter: &dyn Counter = match self {
            SupportedCounter::Duration(counter) => counter,
            SupportedCounter::RequestsDuration(counter) => counter,
            SupportedCounter::RequestsCount(counter) => counter,
        };
        counter.count()
    }
}

trait Counter {
    fn count(&self) -> f64;
}

trait RequestMonitoringCounter: Counter {
    fn on_request(&mut self);
    fn on_response(&mut self);
}

fn duration_to_secs(duration: Duration) -> f64 {
    duration
        .to_std()
        .expect("Duration is bigger than 0")
        .as_secs_f64()
}
