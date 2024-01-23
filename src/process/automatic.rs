use std::{collections::HashMap, ffi::OsString, io::BufReader, path::PathBuf, process::{ExitStatus, Stdio, ExitCode}, sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use subprocess::{Exec, Popen, PopenConfig};
use tokio::{
    sync::{oneshot, Mutex}, runtime::{Builder},
};
use tokio_stream::{wrappers::LinesStream, StreamExt};

use std::io::prelude::*;
use super::Runtime;


#[derive(Clone)]
pub struct Automatic {
    runtime: Arc<tokio::runtime::Runtime>,
}

//TODO parameterize it

static _BASE_DIR: &str = "sd.webui_noxformers";

static _API_HOST: &str = "http://localhost:7861";

static _API_KILL_PATH: &str = "sdapi/v1/server-kill";

static _MODEL_ARG: &str = "--ckpt";

static _DEFAULT_ARGS: &str = 
    "--no-download-sd-model \
    --do-not-download-clip \
    --skip-prepare-environment \
    --skip-install \
    --api \
    --api-log \
    --nowebui \
    --api-server-stop \
    --log-startup \
    --skip-torch-cuda-test \
    --skip-python-version-check \
    --skip-version-check";

static _STARTUP_MSG: &str = "Model loaded in ";

#[async_trait]
impl Runtime for Automatic {
    async fn start(model: Option<PathBuf>) -> anyhow::Result<Automatic> {
        log::info!("Start cmd");
        let base_dir = super::find_exe(_BASE_DIR)?;
        let webui_dir = base_dir.join("webui");
        let system_dir = base_dir.join("system").to_string_lossy().to_string();

        let path_env = std::env::var("PATH").context("Can get PATH env var")?;
        let env:Vec<(OsString, OsString)> = vec![
            ("PATH".into(),        format!("{system_dir}\\git\\bin;{system_dir}\\python;{system_dir}\\python\\Scripts;{path_env}").into()),
            ("PY_LIBS".into(),     format!("{system_dir}\\python\\Scripts\\Lib;{system_dir}\\python\\Scripts\\Lib\\site-packages").into()),
            ("PY_PIP".into(),      format!("{system_dir}\\python\\Scripts").into()),
            ("PIP_INSTALLER_LOCATION".into(),  format!("{system_dir}\\python\\get-pip.py").into()),
            // ("PYTHON",                  "python".into()),
            ("GIT".into(),                     "".into()),
            ("TRANSFORMERS_CACHE".into(),      format!("{system_dir}\\transformers-cache").into()),
            ("SD_WEBUI_RESTART".into(),        "tmp/restart".into()),
            ("ERROR_REPORTING".into(),         "FALSE".into()),
            ("COMMANDLINE_ARGS".into(),        _DEFAULT_ARGS.into())
        ];
        
        // let exe = exe.to_string_lossy().to_string();
        // let work_dir = exe.parent().unwrap();

        let model = model.and_then(format_path).context("No model arg").unwrap();

        let outfile = std::fs::File::create(base_dir.join("auto.log"))?;
        let mut cmd = Popen::create(&[
            "python",
            _MODEL_ARG.into(), 
            &model
        ], PopenConfig {
            stdout: subprocess::Redirection::File(outfile),
            env: Some(env),
            cwd: Some(base_dir.as_os_str().to_os_string()),
             ..Default::default()
        })?;
        

        // let mut cmd = Exec::cmd(&exe,
        //         "--skip-torch-cuda-test",
        //         "--skip-python-version-check",
        //         "--skip-version-check",
        //         _MODEL_ARG,
        //         model
        //     )
            // .dir(work_dir)
            // .stderr_to_stdout()
            // .stdout_path(work_dir.join("automatic.log"));

        let (startup_event_sender, startup_event_receiver) = oneshot::channel::<String>();
        let mut output_handler = OutputHandler::LookingForStartup {
            startup_event_sender,
        };

        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("proces_output_handler")
            .enable_all()
            .build()
            .unwrap();

        runtime.spawn(async move {
            loop {
                match cmd.communicate(None) {
                    Ok((Some(out), None)) => log::info!(">  {out}"),
                    Ok((None, Some(err))) => log::info!("!  {err}"),
                    Ok((Some(out), Some(err))) => log::info!("> {out} - ! {err}"),
                    _ => tokio::time::sleep(Duration::from_secs(1)).await,
                }

            }

            // let reader = cmd.reader()
            //     .map_err(|err| {
            //         log::error!("Failed to spawn process. Err: {err}");
            //         err
            // })
            // .unwrap();
        
            // let mut bufreader = BufReader::with_capacity(60, reader);
            // let mut lines = bufreader.lines();
            // for next_line in lines {
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
