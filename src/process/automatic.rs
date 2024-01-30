pub(crate) mod config;

mod monitor;

use self::config::Config;

use super::Runtime;

use crate::process::automatic::monitor::OutputMonitor;
use anyhow::Context;
use async_trait::async_trait;
use clap::Parser;
use std::pin::Pin;
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
    config: Config,
}

#[async_trait]
impl Runtime for Automatic {
    type CONFIG = Config;

    fn parse_args(args: &[String]) -> anyhow::Result<Self::CONFIG> {
        Ok(Self::CONFIG::try_parse_from(
            std::iter::once(&"".to_string()).chain(args),
        )?)
    }

    async fn start(model: Option<PathBuf>, config: Self::CONFIG) -> anyhow::Result<Automatic> {
        log::info!("Building startup cmd. Config {config:?}");
        let mut cmd = build_cmd(model, &config)?;

        log::info!("Spawning Automatic process");
        let mut child = cmd.kill_on_drop(true).spawn()?;

        let output = output_lines(&mut child)?;

        log::info!("Waiting for Automatic startup");
        let output_monitor = timeout(
            config.startup_timeout,
            OutputMonitor::start(output, config.clone()),
        )
        .await
        .context("Automatic startup timeout.")??;

        log::info!("Automatic has started");
        let child = Arc::new(Mutex::new(child));

        Ok(Self {
            child,
            output_monitor: Arc::new(output_monitor),
            config,
        })
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        log::info!("Stopping Automatic server");
        let client = reqwest::Client::new();
        let url = format!(
            "http://{}:{}/{}",
            self.config.api_host, self.config.api_port, self.config.api_shutdown_path
        );
        if let Err(err) = client.post(url).send().await {
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

fn build_cmd(model: Option<PathBuf>, config: &Config) -> anyhow::Result<Command> {
    let script = super::find_file(&config.startup_script)?;

    let mut cmd = Command::new(&script);

    cmd.args(&config.additional_args);

    if let Some(model) = model.and_then(format_path) {
        cmd.args([&config.model_arg, &model]);
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
mod windows_tests {
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    #[test]
    fn automatic_args() {
        let config = Automatic::parse_args(&[
            "--startup-script".into(),
            "path/run.bat".into(),
            "--api-ping-delay".into(),
            "100ms".into(),
        ])
        .unwrap();
        assert!(config.startup_script == "path/run.bat");
        assert!(config.api_ping_delay == Duration::from_millis(100));
    }
}
