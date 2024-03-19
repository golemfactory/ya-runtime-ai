mod combined;
mod duration;
mod requests;
mod requests_duration;

use std::{str::FromStr, sync::Arc};

use anyhow::bail;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;
use ya_gsb_http_proxy::monitor::RequestsMonitor;

use self::{
    combined::RequestsMonitoringCounters, duration::DurationCounter, requests::RequestsCounter,
    requests_duration::RequestsDurationCounter,
};

type SharedCounters = Arc<RwLock<Vec<SupportedCounter>>>;

#[derive(Clone, Debug)]
pub struct Counters {
    counters: SharedCounters,
    requests_monitor: RequestsMonitoringCounters,
}

impl Counters {
    /// Creates counters from Agreement counter names and starts requests monitoring counters.
    /// Fails on unsupported counter.
    pub fn start(counter_names: &Vec<String>) -> anyhow::Result<Self> {
        let mut counters = Vec::with_capacity(counter_names.len());
        for counter in counter_names {
            let counter = SupportedCounter::from_str(counter)?;
            counters.push(counter);
        }
        let counters = Arc::new(RwLock::new(counters));

        let requests_monitor = RequestsMonitoringCounters::start(counters.clone());

        Ok(Self {
            counters,
            requests_monitor,
        })
    }

    /// Returns usage reported by counters in Agreement specified order.
    /// None if Agreement had no counter names.
    pub async fn current_usage(&self) -> Option<Vec<f64>> {
        let counters = self.counters.read().await;
        let current_usage = counters.iter().map(Counter::count).collect();
        Some(current_usage)
    }

    pub fn requests_monitor(&self) -> impl RequestsMonitor {
        self.requests_monitor.clone()
    }
}

#[derive(Debug)]
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
    fn request_monitoring_counter(&mut self) -> Option<&mut dyn RequestMonitoringCounter> {
        match self {
            SupportedCounter::Duration(_) => None,
            SupportedCounter::RequestsDuration(counter) => Some(counter),
            SupportedCounter::RequestsCount(counter) => Some(counter),
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
    fn on_request(&mut self, request_time: DateTime<Utc>);
    fn on_response(&mut self, response_time: DateTime<Utc>);
}

fn duration_to_secs(duration: Duration) -> f64 {
    duration
        .to_std()
        .expect("Duration is bigger than 0")
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use tokio::task;

    use super::{Counters, SupportedCounter};
    use ya_gsb_http_proxy::monitor::{RequestsMonitor, ResponseMonitor};

    #[tokio::test]
    async fn counters_order_test() {
        let c = Counters::start(&vec![
            "ai-runtime.requests".into(),
            "golem.usage.duration_sec".into(),
            "golem.usage.gpu-sec".into(),
        ])
        .expect("Creates counters");
        let counters = c.counters.read().await;
        assert!(matches!(
            counters.first(),
            Some(&SupportedCounter::RequestsCount(..))
        ));
        assert!(matches!(
            counters.get(1),
            Some(&SupportedCounter::Duration(..))
        ));
        assert!(matches!(
            counters.get(2),
            Some(&SupportedCounter::RequestsDuration(..))
        ));
        assert_eq!(counters.len(), 3);
    }

    #[tokio::test]
    async fn one_counter_test() {
        let counters =
            Counters::start(&vec!["golem.usage.duration_sec".into()]).expect("Creates counters");
        let counters = counters.counters.read().await;
        assert!(matches!(
            counters.first(),
            Some(&SupportedCounter::Duration(..))
        ));
        assert_eq!(counters.len(), 1);
    }

    #[tokio::test]
    async fn zero_counter_error_test() {
        let counters = Counters::start(&vec![]).expect("Creates empty counters collection");
        let counters = counters.counters.read().await;
        assert!(counters.is_empty());
    }

    #[tokio::test]
    async fn overlapping_requests_counter_test() {
        let counters = Counters::start(&vec![
            "golem.usage.duration_sec".into(),
            "ai-runtime.requests".into(),
            "golem.usage.gpu-sec".into(),
        ])
        .expect("Creates counters");

        let test_tasks = task::LocalSet::new();

        let delay = std::time::Duration::from_secs(1);
        let mut requests_monitor = counters.requests_monitor();

        // 3 requests at step 1, 3, and 5.
        test_tasks.spawn_local(async move {
            for i in 0..6 {
                // println!("Step: {i}");
                if (i & 1) == 1 {
                    // odd step number
                    // println!("Short request on step: {i}");
                    let response_monitor = requests_monitor.on_request().await;
                    // println!("Short request on step: {i}. Done.");
                    tokio::time::sleep(delay).await;
                    response_monitor.on_response();
                    // println!("Short request response on step: {i}");
                } else {
                    tokio::time::sleep(delay).await;
                }
            }
        });

        // 1 long request between steps 2 and 4.
        let mut requests_monitor = counters.requests_monitor();
        test_tasks.spawn_local(async move {
            tokio::time::sleep(2 * delay).await;
            // println!("Long request");
            let response_monitor = requests_monitor.on_request().await;
            tokio::time::sleep(delay * 2).await;
            // println!("Long request response.");
            response_monitor.on_response();
        });

        // checking counters
        let c = counters.clone();
        test_tasks.spawn_local(async move {
            assert_eq!(
                vec![0.0, 0.0, 0.0],
                round_vec(c.current_usage().await.unwrap()),
                "Duration 0 sec. Initial assert"
            );

            tokio::time::sleep(delay / 2).await;
            assert_eq!(
                vec![0.5, 0.0, 0.0],
                round_vec(c.current_usage().await.unwrap()),
                r###"
                Duration 0.5 sec. Assert before first short request. 
                Request (GPU) duration counter had not increased.
                "###
            );

            tokio::time::sleep(delay).await;
            assert_eq!(
                vec![1.5, 1.0, 0.5],
                round_vec(c.current_usage().await.unwrap()),
                r###"
                Duration 1.5 sec. Assert after first short request start (before its end).
                Request counter increased and Request duration (GPU) counter increased.
                "###
            );

            tokio::time::sleep(delay).await;
            assert_eq!(
                vec![2.5, 2.0, 1.5],
                round_vec(c.current_usage().await.unwrap()),
                "Duration 2.5 sec. Assert after first short request end and after long request start."
            );

            tokio::time::sleep(delay).await;
            assert_eq!(
                vec![3.5, 3.0, 2.5],
                round_vec(c.current_usage().await.unwrap()),
                r###"
                Duration 3.5 sec. Assert after second short request end and before long request stop. 
                Overlapping requests did not increse Request (GPU) duration counter.
                "###
            );

            tokio::time::sleep(delay).await;
            assert_eq!(
                vec![4.5, 3.0, 3.0],
                round_vec(c.current_usage().await.unwrap()),
                "Duration 4.5 sec. Assert before third short request start and after long request stop"
            );
        });

        test_tasks.await;

        assert_eq!(
            vec![6.0, 4.0, 4.0],
            round_vec(counters.current_usage().await.unwrap()),
            "Duration 6.0 sec. Final assert after third short request end."
        );
    }

    #[tokio::test]
    async fn unhandled_response_event_test() {
        let counters = Counters::start(&vec![
            "golem.usage.duration_sec".into(),
            "ai-runtime.requests".into(),
            "golem.usage.gpu-sec".into(),
        ])
        .expect("Creates counters");

        let test_tasks = task::LocalSet::new();

        let delay = std::time::Duration::from_secs(1);
        let mut requests_monitor = counters.requests_monitor();

        // 3 requests at step 1, 3, and 5.
        test_tasks.spawn_local(async move {
            tokio::time::sleep(delay / 2).await;
            let _response_monitor = requests_monitor.on_request().await;
            tokio::time::sleep(delay).await;
        });

        // checking counters
        let c = counters.clone();
        test_tasks.spawn_local(async move {
            assert_eq!(
                vec![0.0, 0.0, 0.0],
                round_vec(c.current_usage().await.unwrap()),
                "Duration 0 sec. Initial assert"
            );

            tokio::time::sleep(delay).await;
            assert_eq!(
                vec![1.0, 1.0, 0.5],
                round_vec(c.current_usage().await.unwrap()),
                "Duration 1.0 sec. Request started"
            );
        });

        test_tasks.await;

        assert_eq!(
            vec![1.5, 1.0, 1.0],
            round_vec(counters.current_usage().await.unwrap()),
            "Duration 1.0 sec. Request closed on response monitor drop (Response (GPU) duration 1 sec)."
        );
    }

    fn round_vec(vec: Vec<f64>) -> Vec<f64> {
        vec.into_iter().map(|x| round_floor_f64(x, 1)).collect()
    }

    fn round_floor_f64(x: f64, decimals: i32) -> f64 {
        if x == 0.0 {
            return 0.0;
        }
        let y: f64 = 10f64.powi(decimals);
        (x * y).floor() / y
    }
}
