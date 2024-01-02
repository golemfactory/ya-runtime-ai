use std::{path::Path, process::Stdio};

use tokio::{process::Command, io::BufReader, io::AsyncBufReadExt, };

use super::{AiFramework, RuntimeArgs};

#[derive(Clone)]
pub struct Automatic {
    
}

static _STARTUP_SCRIPT: &str = "automatic/run.bat";

impl AiFramework for Automatic {
    fn parse_args(args: &[String]) -> anyhow::Result<super::RuntimeArgs> {
        RuntimeArgs::new(&_STARTUP_SCRIPT.into(), args)
    }

    fn start(args: &super::RuntimeArgs) -> anyhow::Result<tokio::process::Child> {
        log::info!("Start cmd");
        let exe = super::find_exe(_STARTUP_SCRIPT)?;
        let mut cmd = Command::new(&exe);
        let work_dir = exe.parent().unwrap();
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir);
        Ok(cmd.kill_on_drop(true).spawn()?)
    }

    fn run<ReportFn: Fn(super::Usage) + 'static>(stdout: tokio::process::ChildStdout, report_fn: ReportFn) {
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
}
