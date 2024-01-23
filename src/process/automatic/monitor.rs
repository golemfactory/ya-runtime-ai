use super::*;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

static _API_PING_DELAY: Duration = Duration::from_millis(997);

static _STARTUP_MSG: &str = "Model loaded in ";

const _LOG_MESSAGES_W_TRACE_LVL: [&str; 1] = [
    // log generated by API ping task
    "\"GET / HTTP/1.1\" 404 Not Found",
];

#[derive(Clone)]
pub(super) struct OutputMonitor {
    has_started: Arc<Notify>,
    has_stopped: Arc<Notify>,
    #[allow(dead_code)]
    output_task: Arc<JoinHandle<()>>,
    #[allow(dead_code)]
    pinger_task: Arc<JoinHandle<()>>,
}

impl OutputMonitor {
    pub fn start(lines: OutputLines) -> Self {
        let has_started: Arc<Notify> = Default::default();
        let has_stopped: Arc<Notify> = Default::default();
        let output_handler = OutputHandler::LookingForStartup {
            notifier: has_started.clone(),
        };
        let output_task = Arc::new(spawn_output_monitoring(lines, output_handler));
        // Repetitive calling Automatic API triggers flushing Automatic process `stdout`.
        let pinger_task = Arc::new(spawn_api_pinger());
        Self {
            has_started,
            has_stopped,
            output_task,
            pinger_task,
        }
    }

    pub async fn wait_for_startup(&self) {
        self.has_started.notified().await;
    }

    #[allow(dead_code)]
    pub async fn wait_for_shutdown(&self) {
        self.has_stopped.notified().await;
    }
}

fn spawn_output_monitoring(
    mut lines: OutputLines,
    mut output_handler: OutputHandler,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(line) = lines.next().await {
            match line {
                Ok(line) => {
                    output_handler = output_handler.handle(line);
                }
                Err(err) => log::error!("Failed to read line. Err {err}"),
            }
        }
    })
}

/// Repetitive calling Automatic API triggers flushing process `stdout`.
/// It is required to log it, to monitor Automatic startup, and its shutdown.
/// When Automatic is started from console its output gets flushed.
/// Description and solution idea for faced issue https://stackoverflow.com/a/39528785/2608409
fn spawn_api_pinger() -> JoinHandle<()> {
    log::debug!("Starting API pinger");
    let client = Client::new().get(format!("http://{_API_HOST}:{_API_PORT}"));
    tokio::spawn(async move {
        loop {
            let Some(client) = client.try_clone() else {
                log::error!("Unable ping API");
                break;
            };
            log::trace!("Pinging API");
            match client.send().await {
                Ok(response) => log::trace!("Ping respone: {response:?}"),
                Err(err) => log::warn!("Ping failure: {err:?}"),
            };
            tokio::time::sleep(_API_PING_DELAY).await;
        }
    })
}

#[derive(Clone)]
enum OutputHandler {
    LookingForStartup { notifier: Arc<Notify> },
    Logging,
}

impl OutputHandler {
    fn handle(self, line: String) -> Self {
        log_process_output(&line);
        match self {
            Self::LookingForStartup { notifier } => {
                if line.starts_with(_STARTUP_MSG) {
                    notifier.notify_waiters();
                    return Self::Logging;
                }
                Self::LookingForStartup { notifier }
            }
            Self::Logging => self,
        }
    }
}

fn log_process_output(line: &str) {
    for message in _LOG_MESSAGES_W_TRACE_LVL {
        if line.contains(message) {
            log::trace!("> {line}");
            return;
        }
    }
    log::debug!("> {line}");
}
