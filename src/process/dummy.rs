use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};

use super::{AiFramework, RuntimeArgs, Usage};

#[derive(Clone)]
pub struct Dummy {
}

impl Dummy {
}

impl Unpin for Dummy {}

impl AiFramework for Dummy {

    fn parse_args(args: &[String]) -> anyhow::Result<RuntimeArgs> {
        let dummy_filename = dummy_filename();
        RuntimeArgs::new(&dummy_filename, args)
    }

    fn start(args: &RuntimeArgs) -> anyhow::Result<Child> {
        let dummy_filename = dummy_filename();
        let exe = super::find_exe(dummy_filename)?;
        let mut cmd = Command::new(&exe);
        let work_dir = exe.parent().unwrap();
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir)
            .arg("--model")
            .arg(&args.model);
        Ok(cmd.kill_on_drop(true).spawn()?)
    }

    fn run<ReportFn: Fn(Usage) + 'static>(stdout: ChildStdout, _report_fn: ReportFn) {
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
}


fn dummy_filename() -> String { 
    format!("dummy{}", std::env::consts::EXE_SUFFIX)
}
