use chrono::Utc;
use futures::channel::mpsc;
use futures::{future, SinkExt, StreamExt, TryStreamExt};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use ya_client_model::activity::{
    CommandProgress, CommandResult, ExeScriptCommand, ExeScriptCommandResult, RuntimeEvent,
};
use ya_core_model::activity;
use ya_core_model::activity::RpcMessageError;
use ya_service_bus::typed as gsb;

#[derive(Clone, Default)]
pub struct Batches {
    batches: Rc<RefCell<HashMap<String, Rc<RefCell<Batch>>>>>,
}

struct Batch {
    id: String,
    results: Vec<ExeScriptCommandResult>,
    pub events: Broadcast<RuntimeEvent>,
}

#[derive(Clone)]
pub struct BatchRef {
    batch: Rc<RefCell<Batch>>,
}

impl From<&Rc<RefCell<Batch>>> for BatchRef {
    fn from(value: &Rc<RefCell<Batch>>) -> Self {
        BatchRef {
            batch: value.clone(),
        }
    }
}

impl Batches {
    pub fn start_batch(&self, id: &str) -> BatchRef {
        let mut batches = self.batches.borrow_mut();
        let batch = batches
            .entry(id.to_string())
            .or_insert(Rc::new(RefCell::new(Batch {
                id: id.to_string(),
                results: vec![],
                events: Default::default(),
            })));
        BatchRef {
            batch: batch.clone(),
        }
    }

    pub fn results(&self, id: &str) -> Option<Vec<ExeScriptCommandResult>> {
        self.batches
            .borrow()
            .get(id)
            .map(|batch| batch.borrow().results.clone())
    }

    pub fn get_batch(&self, batch_id: &str) -> Option<BatchRef> {
        self.batches.borrow().get(batch_id).map(Into::into).clone()
    }

    pub fn bind_gsb(&self, exe_unit_url: &str) {
        let self_ = self.clone();
        gsb::bind(exe_unit_url, move |exec: activity::GetExecBatchResults| {
            if let Some(result) = self_.results(&exec.batch_id) {
                future::ok(result)
            } else {
                future::err(RpcMessageError::NotFound(format!(
                    "Batch id={}",
                    exec.batch_id
                )))
            }
        });

        let self_ = self.clone();
        gsb::bind_stream(
            exe_unit_url,
            move |exec: activity::StreamExecBatchResults| {
                if let Some(batch) = self_.get_batch(&exec.batch_id) {
                    let mut batch = batch.batch.borrow_mut();
                    Box::pin(batch.events.receiver().left_stream())
                } else {
                    Box::pin(
                        futures::stream::once(future::err(RpcMessageError::NotFound(format!(
                            "Batch id={}",
                            exec.batch_id
                        ))))
                        .right_stream(),
                    )
                }
            },
        );
    }
}

impl BatchRef {
    #[allow(unused)]
    pub fn id(&self) -> String {
        self.batch.borrow().id.clone()
    }

    /// Call on beginning of execution of new command.
    /// Returns index of next command that will be processed.
    pub fn next_command(&self, cmd: &ExeScriptCommand) -> usize {
        let mut batch = self.batch.borrow_mut();
        let index = batch.results.len();
        let event = RuntimeEvent::started(batch.id.clone(), index, cmd.clone());

        batch.events.sender().send(event).ok();
        index
    }

    /// Finish whole batch.
    pub fn finish(&self) {
        let results = &mut self.batch.borrow_mut().results;
        if let Some(last) = results.last_mut() {
            last.is_batch_finished = true;
        } else {
            results.push(ExeScriptCommandResult {
                index: results.len() as u32,
                result: CommandResult::Error,
                stdout: None,
                stderr: None,
                message: None,
                is_batch_finished: true,
                event_date: Utc::now(),
            })
        }
    }

    /// Finish currently executed command with success.
    pub fn ok_result(&self) -> usize {
        self.add_result(CommandResult::Ok, None)
    }

    /// Finish currently executed command with error.
    pub fn err_result(&self, message: Option<String>) -> usize {
        self.add_result(CommandResult::Error, message)
    }

    pub fn update_progress(&self, index: usize, progress: &CommandProgress) {
        let mut batch = self.batch.borrow_mut();
        let event = RuntimeEvent::progress(batch.id.clone(), index, progress.clone());

        batch.events.sender().send(event).ok();
    }

    fn add_result(&self, result: CommandResult, message: Option<String>) -> usize {
        let mut batch = self.batch.borrow_mut();
        let index = batch.results.len();
        batch.results.push(ExeScriptCommandResult {
            index: index as u32,
            result,
            stdout: None,
            stderr: None,
            message: message.clone(),
            is_batch_finished: false,
            event_date: Utc::now(),
        });

        let event = RuntimeEvent::finished(
            batch.id.clone(),
            index,
            match result {
                CommandResult::Ok => 0,
                CommandResult::Error => 1,
            },
            message,
        );
        batch.events.sender().send(event).ok();
        index
    }
}

pub(crate) struct Broadcast<T: Clone> {
    sender: Option<broadcast::Sender<T>>,
}

impl<T: Clone + Send + 'static> Broadcast<T> {
    pub fn initialized(&self) -> bool {
        self.sender.is_some()
    }

    pub fn sender(&mut self) -> &mut broadcast::Sender<T> {
        if !self.initialized() {
            self.initialize();
        }
        self.sender.as_mut().unwrap()
    }

    pub fn receiver(&mut self) -> mpsc::UnboundedReceiver<Result<T, RpcMessageError>> {
        let (tx, rx) = mpsc::unbounded::<Result<T, _>>();
        let mut txc = tx.clone();
        let receiver = self.sender().subscribe();
        tokio::task::spawn_local(async move {
            if let Err(e) = BroadcastStream::new(receiver)
                .map_err(|e| RpcMessageError::Service(e.to_string()))
                .forward(
                    tx.sink_map_err(|e| RpcMessageError::Service(e.to_string()))
                        .with(|v| futures::future::ready(Ok(Ok(v)))),
                )
                .await
            {
                let msg = format!("stream error: {}", e);
                log::error!("Broadcast output error: {}", msg);
                let _ = txc.send(Err::<T, _>(RpcMessageError::Service(msg))).await;
            }
        });
        rx
    }

    fn initialize(&mut self) {
        let (tx, rx) = broadcast::channel(16);
        let receiver = BroadcastStream::new(rx);
        tokio::task::spawn_local(receiver.for_each(|_| async {}));
        self.sender = Some(tx);
    }
}

impl<T: Clone> Default for Broadcast<T> {
    fn default() -> Self {
        Broadcast { sender: None }
    }
}
