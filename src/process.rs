use anyhow::Context;
use async_trait::async_trait;
use bytes::{Buf, BytesMut};
use futures::TryFutureExt;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::{io::BufReader, process::Child};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

use std::cell::RefCell;
use std::env::current_exe;
use std::fmt::Debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::pin::Pin;
use std::process::ExitStatus;
use std::rc::Rc;
use std::task::Poll;

use ya_agreement_utils::OfferTemplate;

use crate::offer_template::{self, gpu_detection};

pub mod automatic;
pub mod dummy;

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

    fn test(config: &Self::CONFIG) -> anyhow::Result<()> {
        gpu_detection(config).map_err(|err| {
            anyhow::anyhow!("Testing runtime failed. Unable to detect GPU. Error: {err}")
        })?;
        Ok(())
    }

    fn offer_template(config: &Self::CONFIG) -> anyhow::Result<OfferTemplate> {
        let mut template = offer_template::template(config)?;
        let gpu = gpu_detection(config).map_err(|err| {
            anyhow::anyhow!("Generating offer template failed. Unable to detect GPU. Error: {err}")
        })?;
        let gpu = serde_json::value::to_value(gpu)?;
        template.set_property("golem.!exp.gap-35.v1.inf.gpu", gpu);
        Ok(template)
    }
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

pub struct LossyLinesCodec {
    max_length: usize,
}

impl Default for LossyLinesCodec {
    fn default() -> Self {
        Self {
            max_length: usize::MAX,
        }
    }
}

pub type OutputLines = Pin<Box<dyn futures::Stream<Item = anyhow::Result<String>> + Send>>;

/// Reads process stdout and stderr using `LossyLinesCodec`
pub fn process_output(child: &mut Child) -> anyhow::Result<OutputLines> {
    let stdout = child
        .stdout
        .take()
        .context("Failed to access process stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("Failed to access process stderr")?;

    let stdout = FramedRead::new(BufReader::new(stdout), LossyLinesCodec::default());
    let stderr = FramedRead::new(BufReader::new(stderr), LossyLinesCodec::default());

    Ok(futures::StreamExt::boxed(stdout.merge(stderr)))
}

/// Decodes lines as UTF-8 (lossly) up to `max_length` characters per line.
impl Decoder for LossyLinesCodec {
    type Item = String;

    type Error = anyhow::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let read_to = std::cmp::min(self.max_length.saturating_add(1), buf.len());
        let new_line_offset = buf[0..read_to].iter().position(|b| *b == b'\n');
        let has_new_line = new_line_offset.is_some();
        let offset = new_line_offset.unwrap_or(read_to);
        let line = buf.split_to(offset);
        if has_new_line {
            // Move cursor pass new line character so next call of `decode` will not read it.
            buf.advance(1);
        }
        let mut line: &[u8] = &line;
        if let Some(&b'\r') = line.last() {
            // Skip carriage return.
            line = &line[..line.len() - 1];
        }
        if line.is_empty() {
            return Ok(None);
        }
        // Process output on Windows is encoded in UTF-16. To avoid OS specific implementation of process output handling the output is lossy converted to UTF-8. 
        // It allows to avoid errors when decoding some Windows error log messages.
        let line = String::from_utf8_lossy(line).to_string();
        Ok(Some(line))
    }
}

#[cfg(test)]
mod tests {

    use test_case::test_case;

    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    use super::LossyLinesCodec;

    #[test_case("foo\nbar\nbaz".as_bytes(), &["foo", "bar", "baz"]; "CL multi line")]
    #[test_case("foo\r\nbar\r\nbaz".as_bytes(), &["foo", "bar", "baz"]; "CRCL multi line")]
    #[test_case("foo".as_bytes(), &["foo"]; "one line")]
    #[test_case("fóó\r\nbąr\r\nbąż".as_bytes(), &["fóó", "bąr", "bąż"];  "diacritics in UTF-8")]
    #[test_case("".as_bytes(), &[]; "empty")]
    #[test_case(&[0x66, 0x6F, 0x80], &["fo�"]; "invalid characters")]
    #[tokio::test]
    async fn lines_codec_test(encoded: &[u8], expected: &[&str]) {
        let mut reader: FramedRead<&[u8], LossyLinesCodec> =
            FramedRead::new(encoded, LossyLinesCodec::default());
        let mut decoded = Vec::new();
        while let Some(line) = reader.next().await {
            match line {
                Ok(line) => decoded.push(line),
                Err(e) => panic!("Error reading line: {}", e),
            }
        }
        let decoded = decoded.iter().map(String::as_str).collect::<Vec<&str>>();
        assert_eq!(expected, decoded.as_slice());
    }
}
