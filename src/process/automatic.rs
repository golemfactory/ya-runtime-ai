use std::{
    process::{ExitStatus, Stdio},
    sync::Arc,
};

use async_trait::async_trait;
use tokio::{
    io::AsyncBufReadExt,
    io::BufReader,
    process::{Child, Command},
    sync::Mutex,
};

use super::{Runtime, RuntimeArgs};

#[derive(Clone)]
pub struct Automatic {
    child: Arc<Mutex<Child>>,
}

static _STARTUP_SCRIPT: &str = "automatic/run.bat";

static _API_HOST: &str = "http://localhost:7861";

static _API_KILL_PATH: &str = "sdapi/v1/server-stop";

#[async_trait]
impl Runtime for Automatic {
    fn parse_args(args: &[String]) -> anyhow::Result<super::RuntimeArgs> {
        RuntimeArgs::new(&_STARTUP_SCRIPT.into(), args)
    }

    fn start(_args: &super::RuntimeArgs) -> anyhow::Result<Automatic> {
        log::info!("Start cmd");
        let exe = super::find_exe(_STARTUP_SCRIPT)?;
        let mut cmd = Command::new(&exe);
        let work_dir = exe.parent().unwrap();
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir);
        let mut child = cmd.kill_on_drop(true).spawn()?;

        let stdout = child.stdout.take();

        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();

                while let Some(line) = reader.next_line().await.unwrap_or_else(|e| {
                    log::debug!("Error reading line from stdout: {}", e);
                    None
                }) {
                    log::debug!("{}", line);
                }
            });
        }

        let child = Arc::new(Mutex::new(child));
        Ok(Self { child })
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        client
            .post(format!("{_API_HOST}/{_API_KILL_PATH}"))
            .send()
            .await?;
        Ok(())
    }

    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let mut child = self.child.lock().await;
        child.wait().await
    }
}
