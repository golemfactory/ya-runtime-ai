use clap::Parser;
use std::cell::RefCell;
use std::env::current_exe;
use std::future::Future;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::pin::{pin, Pin};
use std::process::ExitStatus;
use std::rc::Rc;
use std::task::{Context, Poll};

use tokio::process::*;

pub use self::phoenix::Phoenix;
pub use self::trex::Trex;

mod phoenix;
mod trex;
pub mod win;

#[derive(Default, Clone)]
pub struct Shares {
    pub cnt: u64,
    pub stale_cnt: u64,
    pub invalid_cnt: u64,
    pub new_speed: f64,
    pub difficulty: f64,
}

pub trait MinerEngine {
    fn start(args: &MiningAppArgs) -> anyhow::Result<Child>;

    fn run<ReportFn: Fn(Shares) + 'static>(stdout: ChildStdout, report_fn: ReportFn);
}

const ENV_EXTRA_PARAMS: &str = "EXTRA_MINER_PARAMS";

#[derive(Parser)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub struct MiningAppArgs {
    #[arg(long)]
    pub pool: String,
    #[arg(long)]
    pub wallet: String,
    #[arg(long)]
    pub worker_name: Option<String>,
    #[arg(long)]
    pub password: Option<String>,
}

impl MiningAppArgs {
    pub fn new(args: &[String]) -> anyhow::Result<Self> {
        let me = "gminer".to_string();
        Ok(Self::try_parse_from(std::iter::once(&me).chain(args))?)
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
    Working {
        child: Child,
        shares: u64,
        stale_shares: u64,
        invalid_shares: u64,
        speed: f64,
        diff_share: f64,
    },
    Stopped {
        shares: u64,
        stale_shares: u64,
        invalid_shares: u64,
    },
}

pub fn find_exe(file_name: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let file_name = file_name.as_ref();
    let exe = current_exe()?;
    (|| {
        let f = exe.parent()?.join(file_name);
        if f.exists() {
            return Some(f);
        }
        let f = exe.parent()?.parent()?.join(file_name);
        if f.exists() {
            return Some(f);
        }
        None
    })()
    .ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

impl<T: MinerEngine + Clone + 'static> ProcessController<T> {
    pub fn new() -> Self {
        ProcessController {
            inner: Rc::new(RefCell::new(ProcessControllerInner::Deployed {})),
            _marker: Default::default(),
        }
    }

    pub fn report(&self) -> Option<(u64, u64, u64, f64, f64)> {
        match *self.inner.borrow_mut() {
            ProcessControllerInner::Deployed { .. } => Some((0, 0, 0, 0.0, 0.0)),
            ProcessControllerInner::Working {
                ref shares,
                ref stale_shares,
                ref invalid_shares,
                ref speed,
                ref diff_share,
                ..
            } => Some((*shares, *stale_shares, *invalid_shares, *speed, *diff_share)),
            _ => None,
        }
    }

    pub async fn stop(&self) {
        let (shares, stale_shares, invalid_shares, _, _) = self.report().unwrap_or_default();
        let old = self.inner.replace(ProcessControllerInner::Stopped {
            shares,
            stale_shares,
            invalid_shares,
        });
        if let ProcessControllerInner::Working { mut child, .. } = old {
            let _ = child.kill().await;
        }
    }

    pub async fn start(&self, args: &MiningAppArgs) -> anyhow::Result<()> {
        let mut child = T::start(args)?;

        let opt_stdout = child.stdout.take();
        self.inner.replace(ProcessControllerInner::Working {
            child,
            shares: 0,
            stale_shares: 0,
            invalid_shares: 0,
            speed: 0.0,
            diff_share: 0.0,
        });

        if let Some(stdout) = opt_stdout {
            let me: ProcessController<T> = self.clone();
            T::run(stdout, move |shares| me.new_shares(shares));
        }
        Ok(())
    }

    fn new_shares(&self, new_shares: Shares) {
        if let ProcessControllerInner::Working {
            ref mut shares,
            ref mut stale_shares,
            ref mut invalid_shares,
            ref mut speed,
            ref mut diff_share,
            ..
        } = *self.inner.borrow_mut()
        {
            *shares += new_shares.cnt;
            *stale_shares += new_shares.stale_cnt;
            *invalid_shares += new_shares.invalid_cnt;
            *speed = new_shares.new_speed;
            *diff_share += new_shares.difficulty * 0.001 * new_shares.cnt as f64;
        }
    }
}

impl<T> Future for ProcessController<T> {
    type Output = std::io::Result<ExitStatus>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match *self.inner.borrow_mut() {
            ProcessControllerInner::Working { ref mut child, .. } => {
                let fut = pin!(child.wait());
                fut.poll(cx)
            }
            _ => Poll::Pending,
        }
    }
}
