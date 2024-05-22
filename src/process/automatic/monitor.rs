use crate::process::OutputLines;

use super::*;

use reqwest::Client;
use std::sync::Arc;
use tokio::sync::oneshot::{self};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

pub(super) struct OutputMonitor {
    #[allow(dead_code)]
    output_task: Arc<JoinHandle<()>>,
    #[allow(dead_code)]
    pinger_task: Arc<JoinHandle<()>>,
}

impl OutputMonitor {
    pub async fn start(lines: OutputLines, config: Config) -> anyhow::Result<Self> {
        let (on_startup_tx, on_startup_rx) = oneshot::channel();
        let output_handler = OutputHandler::LookingForStartup {
            on_startup_tx,
            config: config.clone(),
        };
        let output_task = Arc::new(spawn_output_monitoring(lines, output_handler));
        // Repetitive calling Automatic API triggers flushing Automatic process `stdout`.
        let pinger_task = Arc::new(spawn_api_pinger(config.clone()));

        on_startup_rx
            .await
            .context("Automatic failed on startup")??;

        Ok(Self {
            output_task,
            pinger_task,
        })
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
fn spawn_api_pinger(config: Config) -> JoinHandle<()> {
    log::debug!("Starting API pinger");
    let url = format!("http://{}:{}", config.api_host, config.api_port);
    let client = Client::new().get(url);
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
            tokio::time::sleep(config.api_ping_delay).await;
        }
    })
}

enum OutputHandler {
    LookingForStartup {
        //TODO create a custom error type?
        on_startup_tx: oneshot::Sender<anyhow::Result<()>>,
        config: Config,
    },
    Logging {
        config: Config,
    },
}

impl OutputHandler {
    fn handle(self, line: String) -> Self {
        self.log_process_output(&line);
        match self {
            Self::LookingForStartup {
                on_startup_tx,
                config,
            } => {
                if line.starts_with(&config.monitored_startup_msg) {
                    if on_startup_tx.send(Ok(())).is_err() {
                        log::error!("Failed to notify on startup");
                    }
                    return Self::Logging { config };
                } else if line.starts_with(&config.monitored_model_failure_msg) {
                    if on_startup_tx
                        .send(Err(anyhow::anyhow!("Automatic failed to load model")))
                        .is_err()
                    {
                        log::error!("Failed to notify on model loading failure");
                    }
                    log::warn!("Failed to load model");
                    return Self::Logging { config };
                }
                Self::LookingForStartup {
                    on_startup_tx,
                    config,
                }
            }
            // Logging
            _ => self,
        }
    }

    fn log_process_output(&self, line: &str) {
        let config = match self {
            Self::Logging { config } => config,
            Self::LookingForStartup {
                on_startup_tx: _,
                config,
            } => config,
        };
        for message in &config.monitored_msgs_w_trace_lvl {
            if line.contains(message) {
                log::trace!("> {line}");
                return;
            }
        }
        log::debug!("> {line}");
    }
}
