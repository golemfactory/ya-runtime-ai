use crate::process::OutputLines;

use super::*;

use std::sync::Arc;
use tokio::sync::oneshot::{self};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

pub(super) struct OutputMonitor {
    #[allow(dead_code)]
    output_task: Arc<JoinHandle<()>>,
}

impl OutputMonitor {
    pub async fn start(lines: OutputLines, config: Config) -> anyhow::Result<Self> {
        let (on_startup_tx, on_startup_rx) = oneshot::channel();
        let output_handler = OutputHandler::LookingForStartup {
            on_startup_tx,
            config: config.clone(),
        };
        let output_task = Arc::new(spawn_output_monitoring(lines, output_handler));

        on_startup_rx
            .await
            .context("Automatic failed on startup")??;

        Ok(Self { output_task })
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
