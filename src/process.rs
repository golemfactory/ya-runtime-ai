use anyhow::Context;
use async_trait::async_trait;

use futures::TryFutureExt;
use serde::de::DeserializeOwned;

use serde_json::Value;
use std::cell::RefCell;
use std::env::current_exe;
use std::fmt::Debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::{pin, Pin};
use std::process::ExitStatus;
use std::rc::Rc;
use std::task::Poll;

pub mod automatic;
pub mod dummy;
pub mod win;

#[derive(Default, Clone)]
pub struct Usage {
    pub cnt: u64,
}

#[async_trait]
pub(crate) trait Runtime: Sized {
    type CONFIG: RuntimeConfig;

    fn parse_config(config: &Option<Value>) -> anyhow::Result<Self::CONFIG> {
        match config {
            None => Ok(Self::CONFIG::default()),
            Some(config) => Ok(serde_json::from_value(config.clone())?),
        }
    }

    async fn start(mode: Option<PathBuf>, config: Self::CONFIG) -> anyhow::Result<Self>;

    async fn stop(&mut self) -> anyhow::Result<()>;

    async fn wait(&mut self) -> std::io::Result<ExitStatus>;
}

pub(crate) trait RuntimeConfig: DeserializeOwned + Default + Debug + Clone {
    fn gpu_uuid(&self) -> Option<String>;
}

#[derive(Clone)]
pub(crate) struct ProcessController<T: Runtime + 'static> {
    inner: Rc<RefCell<ProcessControllerInner<T>>>,
}

#[allow(clippy::large_enum_variant)]
enum ProcessControllerInner<T: Runtime + 'static> {
    Deployed,
    Working { child: T },
    Stopped,
}

pub fn find_file(file_name: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let exe = current_exe()?;
    let parent_dir = exe
        .parent()
        .context("Unable to get parent dir of {exe:?}")?;
    let file = parent_dir.join(&file_name);
    if file.exists() {
        return Ok(file);
    }
    anyhow::bail!("Unable to get dummy runtime base dir");
}

impl<RUNTIME: Runtime + Clone + 'static> ProcessController<RUNTIME> {
    pub fn new() -> Self {
        ProcessController {
            inner: Rc::new(RefCell::new(ProcessControllerInner::Deployed {})),
        }
    }

    pub fn report(&self) -> Option<()> {
        match *self.inner.borrow_mut() {
            ProcessControllerInner::Deployed { .. } => Some(()),
            ProcessControllerInner::Working { .. } => Some(()),
            _ => None,
        }
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        let () = self.report().unwrap_or_default();
        let old = self.inner.replace(ProcessControllerInner::Stopped {});
        if let ProcessControllerInner::Working { mut child, .. } = old {
            return child.stop().await;
        }
        Ok(())
    }

    pub async fn start(
        &self,
        model: Option<PathBuf>,
        config: RUNTIME::CONFIG,
    ) -> anyhow::Result<()> {
        let child = RUNTIME::start(model, config)
            .inspect_err(|err| log::error!("Failed to start process. Err {err}"))
            .await?;

        self.inner
            .replace(ProcessControllerInner::Working { child });

        Ok(())
    }
}

impl<T: Runtime> Future for ProcessController<T> {
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
