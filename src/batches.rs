use chrono::Utc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ya_client_model::activity::{CommandProgress, CommandResult, ExeScriptCommandResult};

#[derive(Clone, Default)]
pub struct Batches {
    pub results: Rc<RefCell<HashMap<String, Vec<ExeScriptCommandResult>>>>,
}

#[derive(Clone)]
pub struct Batch {
    id: String,
    pub(crate) results: Rc<RefCell<HashMap<String, Vec<ExeScriptCommandResult>>>>,
}

impl Batches {
    pub fn start_batch(&self, id: &str) -> Batch {
        let _ = self
            .results
            .borrow_mut()
            .entry(id.to_string())
            .or_insert(vec![]);
        Batch {
            id: id.to_string(),
            results: self.results.clone(),
        }
    }

    pub fn results(&self, id: &str) -> Option<Vec<ExeScriptCommandResult>> {
        self.results.borrow().get(id).cloned()
    }
}

impl Batch {
    #[allow(unused)]
    pub fn id(&self) -> String {
        self.id.clone()
    }
    pub fn finish(&self) {
        if let Some(results) = self.results.borrow_mut().get_mut(&self.id) {
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
    }

    pub fn ok_result(&self) -> usize {
        self.add_result(CommandResult::Ok, None)
    }
    pub fn err_result(&self, message: Option<String>) -> usize {
        self.add_result(CommandResult::Ok, message)
    }

    pub fn update_progress(&self, index: usize, progress: &CommandProgress) {
        if let Some(results) = self.results.borrow_mut().get_mut(&self.id) {
            if let Some(result) = results.get_mut(index) {
                if let Ok(message) = serde_json::to_string(&progress) {
                    result.message = Some(message);
                }
            }
        }
    }
    fn add_result(&self, result: CommandResult, message: Option<String>) -> usize {
        if let Some(results) = self.results.borrow_mut().get_mut(&self.id) {
            let index = results.len() as u32;
            results.push(ExeScriptCommandResult {
                index,
                result,
                stdout: None,
                stderr: None,
                message,
                is_batch_finished: false,
                event_date: Utc::now(),
            });
            return index as usize;
        }
        0
    }
}
