use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use ya_agreement_utils::OfferTemplate;

use crate::offer_template;

use super::{Runtime, RuntimeConfig};

#[derive(Clone)]
pub struct Dummy {
    child: Arc<Mutex<Child>>,
}

fn dummy_filename() -> String {
    format!("dummy{}", std::env::consts::EXE_SUFFIX)
}

#[derive(Deserialize, Clone, Debug, Default)]
pub(crate) struct Config {
    #[allow(dead_code)]
    pub dummy_arg: Option<String>,
}

impl RuntimeConfig for Config {
    fn gpu_uuid(&self) -> Option<String> {
        None
    }
}

#[async_trait]
impl Runtime for Dummy {
    type CONFIG = Config;

    async fn start(model: Option<PathBuf>, _config: Self::CONFIG) -> anyhow::Result<Dummy> {
        let dummy_filename = dummy_filename();
        let exe = super::find_file(dummy_filename)?;
        let mut cmd = Command::new(&exe);
        let work_dir = exe.parent().unwrap();
        if let Some(model) = model {
            cmd.args(["--model", &model.to_string_lossy()]);
        }
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir);
        let mut child = cmd.kill_on_drop(true).spawn()?;

        let stdout = child.stdout.take();
        if let Some(stdout) = stdout {
            tokio::task::spawn_local(async move {
                let mut stdout = BufReader::new(stdout);
                loop {
                    let mut line_buf = String::new();
                    match stdout.read_line(&mut line_buf).await {
                        Err(e) => {
                            log::error!("no line: {}", e);
                            break;
                        }
                        Ok(0) => break,
                        Ok(_) => (),
                    }
                    let line = line_buf.trim_end();
                    log::info!("dummy response: {line}");
                }
            });
        }

        let child = Arc::new(Mutex::new(child));
        Ok(Self { child })
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        log::info!("Stopping runtime");
        Ok(())
    }

    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let mut child = self.child.lock().await;
        child.wait().await
    }

    fn test(_config: &Self::CONFIG) -> anyhow::Result<()> {
        Ok(())
    }

    fn offer_template(config: &Self::CONFIG) -> anyhow::Result<OfferTemplate> {
        offer_template::template(config)
    }
}
