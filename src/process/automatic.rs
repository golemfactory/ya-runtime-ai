mod monitor;

use super::Runtime;
use crate::process::automatic::monitor::OutputMonitor;
use anyhow::Context;
use async_trait::async_trait;
use std::pin::Pin;
use std::time::Duration;
use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio},
    sync::Arc,
};
use tokio::{
    io::AsyncBufReadExt,
    io::BufReader,
    process::{Child, Command},
    sync::Mutex,
    time::timeout,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};

#[derive(Clone)]
pub struct Automatic {
    child: Arc<Mutex<Child>>,
    #[allow(dead_code)]
    output_monitor: Arc<monitor::OutputMonitor>,
}

//TODO parameterize it

static _STARTUP_SCRIPT: &str = "sd.webui_noxformers/run.bat";

static _API_PORT: u16 = 7861;

static _API_HOST: &str = "localhost";

static _API_KILL_PATH: &str = "sdapi/v1/server-kill";

static _MODEL_ARG: &str = "--ckpt";

static _SKIP_TEST_ARGS: [&str; 3] = [
    "--skip-torch-cuda-test",
    "--skip-python-version-check",
    "--skip-version-check",
];

const _STARTUP_TIMEOUT: Duration = Duration::from_secs(90);

#[async_trait]
impl Runtime for Automatic {
    async fn start(model: Option<PathBuf>) -> anyhow::Result<Automatic> {
        log::info!("Building startup cmd");
        let mut cmd = build_cmd(model)?;

        log::info!("Spawning Automatic process");
        let mut child = cmd.kill_on_drop(true).spawn()?;

        let output = output_lines(&mut child)?;

        log::info!("Waiting for Automatic startup");
        let output_monitor = timeout(_STARTUP_TIMEOUT, OutputMonitor::start(output))
            .await
            .context("Automatic startup timeout.")??;

        log::info!("Automatic has started");
        let child = Arc::new(Mutex::new(child));

        Ok(Self {
            child,
            output_monitor: Arc::new(output_monitor),
        })
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        log::info!("Stopping Automatic server");
        let client = reqwest::Client::new();
        if let Err(err) = client
            .post(format!("http://{_API_HOST}:{_API_PORT}/{_API_KILL_PATH}"))
            .send()
            .await
        {
            log::warn!("Automatic stop request failed. Err {err}");
        }
        Ok(())
    }

    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let mut child = self.child.lock().await;
        let res = child.wait().await;
        log::debug!("Automatic process has stopped");
        res
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
