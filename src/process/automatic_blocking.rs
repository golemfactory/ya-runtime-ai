use std::{
    path::PathBuf,
    process::{ExitStatus, Stdio, Command, Child},
    sync::{Arc, Condvar, Mutex}, io::{BufReader, BufRead},
};

use anyhow::Context;
use async_trait::async_trait;

use super::Runtime;

#[derive(Clone)]
pub struct Automatic {
    child: Arc<Mutex<Child>>,
}

//TODO parameterize it

static _STARTUP_SCRIPT: &str = "sd.webui_noxformers/run.bat";

static _API_HOST: &str = "http://localhost:7861";

static _API_KILL_PATH: &str = "sdapi/v1/server-kill";

static _MODEL_ARG: &str = "--ckpt";

static _SKIP_TEST_ARGS: [&str; 3] = [
    "--skip-torch-cuda-test",
    "--skip-python-version-check",
    "--skip-version-check",
];

static _STARTUP_MSG: &str = "Model loaded in ";

#[async_trait]
impl Runtime for Automatic {
    fn start(model: Option<PathBuf>) -> anyhow::Result<Automatic> {
        log::info!("Start cmd");
        let exe = super::find_exe(_STARTUP_SCRIPT)?;

        let mut cmd = Command::new(&exe);
        cmd.args(_SKIP_TEST_ARGS);
        
        if let Some(model) = model.and_then(format_path) {
            cmd.args([_MODEL_ARG, &model]);
        } else {
            log::warn!("No model arg");
        }

        let work_dir = exe.parent().unwrap();
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .current_dir(work_dir);

        let mut child = cmd.spawn()?;

        let stdout = child.stdout.take().context("Can listen on Automatic stdout")?;
        // let stderr = child.stderr.take().context("Can listen on Automatic stdout")?;

        let event_lock = Arc::new((Mutex::new(false), Condvar::new()));

        let mut output_handler = OutputHandler::LookingForStartup { event_lock: event_lock.clone() };

        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);

            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        match output_handler.handle(line) {
                            Ok(new_handler) => output_handler = new_handler,
                            Err(err) => {
                                log::error!("Failed to handle process output. Err {err}");
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("Failed to read Automatic stdout. Err {err}");
                        break;
                    }
                }
            }
        });

        let (event_lock, cvar) = &*event_lock;
        let mut started = event_lock.lock().unwrap();
        while !*started {
            started = cvar.wait(started).unwrap();
        }

        log::debug!("Automatic has started");

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
        let mut child = self.child.lock().unwrap();
        child.wait()
    }
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

enum OutputHandler {
    LookingForStartup {
        event_lock: Arc<(Mutex<bool>, Condvar)>,
    },
    Logging,
}

impl OutputHandler {
    fn handle(self, line: String) -> Result<OutputHandler, String> {
        log::debug!("> {line}");
        match self {
            Self::LookingForStartup {
                event_lock,
            } => {
                if line.starts_with(_STARTUP_MSG) {
                    let (lock, cvar) = &*event_lock;
                    let mut started = lock.lock().unwrap();
                    *started = true;
                    // We notify the condvar that the value has changed.
                    cvar.notify_one();
                    return Ok(Self::Logging);
                }
                Ok(Self::LookingForStartup { event_lock })
            }
            Self::Logging => {
                Ok(self)
            }
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
