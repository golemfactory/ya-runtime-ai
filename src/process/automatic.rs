use std::pin::Pin;
use std::time::Duration;
use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
    sync::Arc,
};

use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;
use tokio::process::ChildStdout;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::{
    io::AsyncBufReadExt,
    io::BufReader,
    process::{Child, Command},
    sync::Mutex,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};

use super::Runtime;

#[derive(Clone)]
pub struct Automatic {
    child: Arc<Mutex<Child>>,
    output_monitor: OutputMonitor,
}

//TODO parameterize it

static _STARTUP_SCRIPT: &str = "sd.webui_noxformers/run.bat";

static _API_PORT: u16 = 7861;

static _API_HOST: &str = "localhost";

static _API_KILL_PATH: &str = "sdapi/v1/server-kill";

static _API_PING_DELAY: Duration = Duration::from_millis(1_000);

static _MODEL_ARG: &str = "--ckpt";

static _SKIP_TEST_ARGS: [&str; 3] = [
    "--skip-torch-cuda-test",
    "--skip-python-version-check",
    "--skip-version-check",
];

static _STARTUP_MSG: &str = "Model loaded in ";

#[async_trait]
impl Runtime for Automatic {
    async fn start(model: Option<PathBuf>) -> anyhow::Result<Automatic> {
        log::info!("Building startup cmd");
        let mut cmd = build_cmd(model)?;

        log::info!("Spawning Automatic process");
        let mut child = cmd.kill_on_drop(true).spawn()?;

        let output = output_lines(&mut child)?;

        log::info!("Starting monitoring Automatic output");
        let output_monitor = OutputMonitor::start(output);

        log::info!("Waiting for Automatic startup");
        output_monitor.wait_for_startup().await;

        log::info!("Automatic has started");
        let child = Arc::new(Mutex::new(child));
        Ok(Self {
            child,
            output_monitor,
        })
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        log::info!("Stopping automatic server");
        let client = reqwest::Client::new();
        client
            .post(format!("http://{_API_HOST}:{_API_PORT}/{_API_KILL_PATH}"))
            .send()
            .await?;
        Ok(())
    }

    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let mut child = self.child.lock().await;
        child.wait().await
    }
}

fn build_cmd(model: Option<PathBuf>) -> anyhow::Result<Command> {
    let script = super::find_file(_STARTUP_SCRIPT)?;

    let mut cmd = Command::new(&script);

    cmd.args(_SKIP_TEST_ARGS);

    if let Some(model) = model.and_then(format_path) {
        cmd.args([_MODEL_ARG, &model]);
    } else {
        log::warn!("No model arg");
    }

    let work_dir = script.parent().unwrap();
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .current_dir(work_dir);
    Ok(cmd)
}

// Automatic needs following ckpt-dir format: C:\\some/path
#[cfg(target_family = "windows")]
fn format_path(path: std::path::PathBuf) -> Option<String> {
    use std::{collections::VecDeque, ffi::OsStr, path::Path};

    if path.has_root() {
        let mut path_parts = VecDeque::new();
        let mut dir = Some(path.as_path());
        while let Some(name) = dir.and_then(Path::file_name).and_then(OsStr::to_str) {
            path_parts.push_front(name);
            dir = dir.and_then(Path::parent);
        }
        if let Some(disk) = dir.and_then(Path::to_str) {
            let relative_path = Into::<Vec<&str>>::into(path_parts).join("/");
            return Some(format!("{disk}\\{relative_path}"));
        }
    }
    log::error!("Unable to correctly format path: {path:?}");
    None
}

#[cfg(target_family = "unix")]
fn format_path(path: std::path::PathBuf) -> Option<String> {
    path.to_str().map(str::to_string)
}

type OutputLines = Pin<Box<dyn futures::Stream<Item = std::io::Result<String>> + Send>>;

fn output_lines(child: &mut Child) -> anyhow::Result<OutputLines> {
    let stdout = child
        .stdout
        .take()
        .context("Failed to read Automatic stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("Failed to read Automatic stderr")?;

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());
    Ok(futures::StreamExt::boxed(stdout.merge(stderr)))
}

#[derive(Clone)]
struct OutputMonitor {
    has_started: Arc<Notify>,
    has_stopped: Arc<Notify>,
    output_task: Arc<JoinHandle<()>>,
    pinger_task: Arc<JoinHandle<()>>,
}

impl OutputMonitor {
    pub fn start(lines: OutputLines) -> Self {
        let has_started: Arc<Notify> = Default::default();
        let has_stopped: Arc<Notify> = Default::default();
        let output_handler = OutputHandler::LookingForStartup {
            notifier: has_started.clone(),
        };
        let output_task = Arc::new(Self::spawn_output_monitoring(lines, output_handler));
        let pinger_task = Arc::new(Self::spawn_api_pinger());
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

    /*
    pub async fn wait_for_shutdown(&self) {
        self.has_stopped.notified().await;
    }
    */

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

    fn spawn_api_pinger() -> JoinHandle<()> {
        log::debug!("Starting API pinger");
        let client = Client::new().get(format!("http://{_API_HOST}:{_API_PORT}"));
        tokio::spawn(async move {
            loop {
                let Some(client) = client.try_clone() else {
                    log::error!("Unable ping API");
                    break;
                };
                log::debug!("Pinging API");
                match client.send().await {
                    Ok(response) => log::debug!("? Ping respone: {response:?}"),
                    Err(err) => log::debug!("? Ping error: {err:?}"),
                };
                tokio::time::sleep(_API_PING_DELAY).await;
            }
        })
    }
}

#[derive(Clone)]
enum OutputHandler {
    LookingForStartup { notifier: Arc<Notify> },
    Logging,
}

impl OutputHandler {
    fn handle(self, line: String) -> Self {
        log::debug!("> {line}");
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

#[cfg(target_family = "windows")]
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn ckpt_dir_test() {
        let path = PathBuf::from(Path::new("C:\\my\\model\\model.ckpt"));
        assert_eq!(
            format_path(path),
            Some("C:\\\\my/model/model.ckpt".to_string())
        );
    }
}
