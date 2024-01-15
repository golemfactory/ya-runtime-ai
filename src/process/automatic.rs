use std::{
    ffi::OsStr,
    path::PathBuf,
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

use super::Runtime;

#[derive(Clone)]
pub struct Automatic {
    child: Arc<Mutex<Child>>,
}

static _STARTUP_SCRIPT: &str = "automatic/run.bat";

static _API_HOST: &str = "http://localhost:7861";

static _API_KILL_PATH: &str = "sdapi/v1/server-stop";

static _MODEL_ARG: &str = "--ckpt";

static _MODEL_DIR_ARG: &str = "--ckpt-dir";

#[async_trait]
impl Runtime for Automatic {
    fn start(model: Option<PathBuf>) -> anyhow::Result<Automatic> {
        log::info!("Start cmd");
        let exe = super::find_exe(_STARTUP_SCRIPT)?;

        let mut cmd = Command::new(&exe);

        if let Some(model) = model {
            let ckpt_dir = model.parent().and_then(ckpt_dir);
            let model_file = model.file_name().and_then(OsStr::to_str);
            if let (Some(ckpt_dir), Some(model_file)) = (ckpt_dir, model_file) {
                cmd.args([_MODEL_DIR_ARG, &ckpt_dir, _MODEL_ARG, model_file]);
            }
        }

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
        log::info!("Stopping automatic server");
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

// Automatic needs following ckpt-dir format: C:\\some/path
#[cfg(target_family = "windows")]
fn ckpt_dir(ckpt_dir: &std::path::Path) -> Option<String> {
    use std::{collections::VecDeque, path::Path};

    if ckpt_dir.has_root() {
        let mut path_parts = VecDeque::new();
        let mut dir = Some(ckpt_dir);
        while let Some(name) = dir.and_then(Path::file_name).and_then(OsStr::to_str) {
            path_parts.push_front(name);
            dir = dir.and_then(Path::parent);
        }
        if let Some(disk) = dir.and_then(Path::to_str) {
            let relative_path = Into::<Vec<&str>>::into(path_parts).join("/");
            return Some(format!("{disk}\\{relative_path}"));
        }
    }
    log::error!("Unable to build ckpt_dir in correct format from path: {ckpt_dir:?}");
    None
}

#[cfg(target_family = "unix")]
fn ckpt_dir(ckpt_dir: &std::path::Path) -> Option<String> {
    ckpt_dir.to_str().map(str::to_string)
}

#[cfg(target_family = "windows")]
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn ckpt_dir_test() {
        let path = Path::new("C:\\my\\model\\dir");
        assert_eq!(ckpt_dir(path), Some("C:\\\\my/model/dir".to_string()));
    }
}
