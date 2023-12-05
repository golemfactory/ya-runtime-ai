use anyhow::Context;
use clap::Parser;
use std::cell::RefCell;
use std::env::current_exe;
use std::future::Future;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::pin::{pin, Pin};
use std::process::ExitStatus;
use std::rc::Rc;
use std::task::Poll;

use tokio::process::*;

pub mod dummy;
pub mod win;

#[derive(Default, Clone)]
pub struct Usage {
    pub cnt: u64,
}

pub trait AiFramework {
    fn parse_args(args: &[String]) -> anyhow::Result<RuntimeArgs>;

    fn start(args: &RuntimeArgs) -> anyhow::Result<Child>;

    fn run<ReportFn: Fn(Usage) + 'static>(stdout: ChildStdout, report_fn: ReportFn);
}

#[derive(Parser)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub struct RuntimeArgs {
    #[arg(long)]
    pub model: String,
}

impl RuntimeArgs {
    pub fn new(cmd: &String, args: &[String]) -> anyhow::Result<Self> {
        Ok(Self::try_parse_from(std::iter::once(cmd).chain(args))?)
    }
}

#[derive(Clone)]
pub struct ProcessController<T> {
    inner: Rc<RefCell<ProcessControllerInner>>,
    _marker: PhantomData<T>,
}

#[allow(clippy::large_enum_variant)]
enum ProcessControllerInner {
    Deployed {},
    Working { child: Child },
    Stopped {},
}

pub fn find_exe(file_name: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let exe = current_exe()?;
    let parent_dir = exe
        .parent()
        .context("Unable to get parent dir of {exe:?}")?;
    let exe_file = parent_dir.join(&file_name);
    if exe_file.exists() {
        return Ok(exe_file);
    }
    anyhow::bail!("Unable to get dummy runtime base dir");
}

impl<T: AiFramework + Clone + 'static> ProcessController<T> {
    pub fn new() -> Self {
        ProcessController {
            inner: Rc::new(RefCell::new(ProcessControllerInner::Deployed {})),
            _marker: Default::default(),
        }
    }

    pub fn report(&self) -> Option<()> {
        match *self.inner.borrow_mut() {
            ProcessControllerInner::Deployed { .. } => Some(()),
            ProcessControllerInner::Working { .. } => Some(()),
            _ => None,
        }
    }

    pub async fn stop(&self) {
        let () = self.report().unwrap_or_default();
        let old = self.inner.replace(ProcessControllerInner::Stopped {});
        if let ProcessControllerInner::Working { mut child, .. } = old {
            let _ = child.kill().await;
        }
    }

    pub async fn start(&self, args: &RuntimeArgs) -> anyhow::Result<()> {
        let mut child = T::start(args)?;

        let opt_stdout = child.stdout.take();
        self.inner
            .replace(ProcessControllerInner::Working { child });

        if let Some(stdout) = opt_stdout {
            let _me: ProcessController<T> = self.clone();
            T::run(stdout, move |_| {});
        }
        Ok(())
    }
}

impl<T> Future for ProcessController<T> {
    type Output = std::io::Result<ExitStatus>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match *self.inner.borrow_mut() {
            ProcessControllerInner::Working { ref mut child, .. } => {
                let fut = pin!(child.wait());
                fut.poll(cx)
            }
            _ => Poll::Pending,
        }
    }
}
