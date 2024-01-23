use std::{path::PathBuf, process::{ExitStatus, Stdio, ExitCode, Command}, sync::Arc, time::Duration, io::{Read, Write}};

use anyhow::Context;
use async_trait::async_trait;
use tokio::{
    sync::{oneshot, Mutex}, runtime::{Builder},
};
use tokio_stream::{wrappers::LinesStream, StreamExt};

use super::Runtime;

#[derive(Clone)]
pub struct Automatic {
    runtime: Arc<tokio::runtime::Runtime>,
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
    async fn start(model: Option<PathBuf>) -> anyhow::Result<Automatic> {
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
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir);

        let (startup_event_sender, startup_event_receiver) = oneshot::channel::<String>();
        let mut output_handler = OutputHandler::LookingForStartup {
            startup_event_sender,
        };

        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("proces_output_handler")
            // .enable_all()
            .build()
            .unwrap();

        runtime.spawn(async move {
            fn communicate(
                mut stream: impl Read,
            ) -> std::io::Result<()> {
                let mut buf = [0u8; 10];
                loop {
                    let num_read = stream.read_exact(&mut buf)?;
                    // if num_read == 0 {
                    //     break;
                    // }
        
                    // let buf = &buf[..num_read];
                    let txt = std::str::from_utf8(&buf).expect("Can parse buf");
                    log::debug!("> {txt}");
                }
        
                Ok(())
            }
            
            let mut child = cmd.spawn()
                .map_err(|err| { log::error!("Failed to spawn process. Err: {err}"); err})
                .unwrap();

            let child_out = std::mem::take(&mut child.stdout).expect("cannot attach to child stdout");
            let child_err = std::mem::take(&mut child.stderr).expect("cannot attach to child stderr");

            let thread_out = std::thread::spawn(move || {
                communicate(child_out)
                    .expect("error communicating with child stdout")
            });
            let thread_err = std::thread::spawn(move || {
                communicate(child_err)
                    .expect("error communicating with child stderr")
            });

            thread_out.join().unwrap();
            thread_err.join().unwrap();

            // let stdout = child
            //     .stdout
            //     .take()
            //     .context("Failed to read Automatic stdout")
            //         .map_err(|err| { log::error!("Failed to get stdout. Err: {err}"); err})
            //         .unwrap();
            // let stderr = child
            //     .stderr
            //     .take()
            //     .context("Failed to read Automatic stderr")
            //     .map_err(|err| { log::error!("Failed to get stderr. Err: {err}"); err})
            //     .unwrap();

            // let stdout = LinesStream::new(BufReader::new(stdout).lines());
            // let stderr = LinesStream::new(BufReader::new(stderr).lines());
            // let mut out = StreamExt::merge(stdout, stderr);

            // while let Some(next_line) = out.next().await {
            //     match next_line {
            //         Ok(line) => {
            //             match output_handler.handle(line) {
            //                 Ok(handler) => { output_handler = handler },
            //                 Err(err) => {
            //                     log::error!("Failed to handle process output line. Err {err}");
            //                     break;
            //                 }
            //             }
            //         },
            //         Err(err) => {
            //             log::error!("Failed to handle process output. Err {err}");
            //             break;
            //         }
            //     }
            // }
        });

        log::info!("Waiting for automatic startup.");
        _ = startup_event_receiver.await?;
        log::info!("Automatic has started.");

        let runtime = Arc::new(runtime);
        Ok(Self { runtime })
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
        // let mut child = self.
        // self.runtime.wait().await
        tokio::time::sleep(Duration::from_secs(10000)).await;
        std::io::Result::Ok(Default::default())
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
        startup_event_sender: oneshot::Sender<String>,
    },
    Logging,
}

impl OutputHandler {
    fn handle(self, line: String) -> Result<OutputHandler, String> {
        log::debug!("> {line}");
        match self {
            Self::LookingForStartup {
                startup_event_sender,
            } => {
                if line.starts_with(_STARTUP_MSG) {
                    startup_event_sender.send(line)?;
                    return Ok(Self::Logging);
                }
                Ok(Self::LookingForStartup {
                    startup_event_sender,
                })
            }
            Self::Logging => Ok(self),
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
