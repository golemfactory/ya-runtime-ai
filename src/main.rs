#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::pin::pin;
use std::rc::Rc;
use std::time::Duration;
use std::{env, io};

use chrono::Utc;
use clap::Parser;
use futures::prelude::*;
use ya_client_model::activity::activity_state::*;
use ya_client_model::activity::ExeScriptCommand;
use ya_client_model::activity::{ActivityUsage, CommandResult, ExeScriptCommandResult};
use ya_core_model::activity;
use ya_core_model::activity::RpcMessageError;
use ya_service_bus::typed as gsb;

use crate::agreement::AgreementDesc;
use crate::cli::*;
use crate::logger::*;
use crate::process::ProcessController;

mod agreement;
mod cli;
mod logger;
mod offer_template;
mod process;

async fn send_state(
    report_url: &str,
    activity_id: &str,
    new_state: ActivityState,
) -> anyhow::Result<()> {
    Ok(gsb::service(report_url)
        .call(activity::local::SetState::new(
            activity_id.into(),
            new_state,
            None,
        ))
        .await??)
}

async fn activity_loop<T: process::Runtime + Clone + Unpin + 'static>(
    report_url: &str,
    activity_id: &str,
    mut process: ProcessController<T>,
    agreement: AgreementDesc,
) -> anyhow::Result<()> {
    let report_service = gsb::service(report_url);
    let start = Utc::now();
    let mut current_usage = agreement.clean_usage_vector();
    let duration_idx = agreement.resolve_counter("golem.usage.duration_sec");

    while let Some(()) = process.report() {
        let now = Utc::now();
        let duration = now - start;

        if let Some(idx) = duration_idx {
            current_usage[idx] = duration.to_std()?.as_secs_f64();
        }
        match report_service
            .call(activity::local::SetUsage {
                activity_id: activity_id.to_string(),
                usage: ActivityUsage {
                    current_usage: Some(current_usage.clone()),
                    timestamp: now.timestamp(),
                },
                timeout: None,
            })
            .await
        {
            Ok(Ok(())) => log::debug!("Successfully sent activity usage message"),
            Ok(Err(rpc_message_error)) => log::error!("rpcMessageError : {:?}", rpc_message_error),
            Err(err) => log::error!("other error : {:?}", err),
        }
        log::debug!("Looping2 ...");

        let sleep = pin!(actix_rt::time::sleep(Duration::from_secs(1)));
        process = match future::select(sleep, process).await {
            future::Either::Left((_, p)) => p,
            future::Either::Right((status, _)) => {
                let _err = report_service
                    .call(activity::local::SetState {
                        activity_id: activity_id.to_string(),
                        state: ActivityState {
                            state: StatePair::from(State::Terminated),
                            reason: Some("process exit".to_string()),
                            error_message: Some(format!("status: {:?}", status)),
                        },
                        timeout: None,
                        credentials: None,
                    })
                    .await;
                log::error!("process exit: {:?}", status);
                anyhow::bail!("Runtime exited")
            }
        }
    }
    Ok(())
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |e| {
        log::error!("AI Runtime panic: {e}");
        panic_hook(e)
    }));

    if let Err(error) = start_file_logger() {
        start_logger().expect("Failed to start logging");
        log::warn!("Using fallback logging due to an error: {:?}", error);
    };

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            log::error!("Failed to parse CLI: {}", err);
            err.exit();
        }
    };

    match cli.runtime.to_lowercase().as_str() {
        "dummy" => run::<process::dummy::Dummy>(cli).await,
        _ => {
            let err = anyhow::format_err!("Unsupported framework {}", cli.runtime);
            log::error!("{}", err);
            anyhow::bail!(err)
        }
    }
}

async fn run<T: process::Runtime + Clone + Unpin + 'static>(cli: Cli) -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let args: Vec<String> = env::args().collect();

    let (exe_unit_url, report_url, activity_id, agreement_path) = match &cli.command {
        /*
        Command::FromFile {
            report_url,
            service_id,
            input,
            args,
        } => {
            let contents = std::fs::read_to_string(input).map_err(|e| {
                anyhow::anyhow!("Cannot read commands from file {}: {e}", input.display())
            })?;
            let contents = serde_json::from_str(&contents).map_err(|e| {
                anyhow::anyhow!(
                    "Cannot deserialize commands from file {}: {e}",
                    input.display(),
                )
            })?;
            //TODO use file content
            (
                ya_core_model::activity::exeunit::bus_id(&service_id),
                report_url.unwrap(),
                service_id.unwrap(),
                args.agreement,
            )
        }
         */
        Command::ServiceBus {
            service_id,
            report_url,
            args,
            ..
        } => (
            ya_core_model::activity::exeunit::bus_id(service_id),
            report_url,
            service_id,
            &args.agreement,
        ),
        Command::OfferTemplate => {
            let template = offer_template::template(cli.runtime)?;
            io::stdout().write_all(template.as_ref())?;
            return Ok(());
        }
        Command::Test => {
            // Test
            return Ok(());
        }
    };

    log::info!("{:?}", args);
    log::info!("CLI args: {:?}", &cli);
    log::info!("Binding to GSB ...");

    let process_controller = process::ProcessController::<T>::new();
    let agreement = AgreementDesc::load(agreement_path)?;
    let activity_pinger = activity_loop(
        report_url,
        activity_id,
        process_controller.clone(),
        agreement,
    );
    #[cfg(target_os = "windows")]
    let _job = process::win::JobObject::new()?;

    {
        let report_url = report_url.clone();
        let activity_id = activity_id.clone();
        let batch: Rc<RefCell<HashMap<String, Vec<ExeScriptCommandResult>>>> = Default::default();
        let batch_results = batch.clone();

        gsb::bind(&exe_unit_url, move |exec: activity::Exec| {
            let report_url = report_url.clone();
            let activity_id = activity_id.clone();
            let process_controller = process_controller.clone();
            let batch = batch.clone();
            async move {
                let mut result = Vec::new();
                for exe in &exec.exe_script {
                    match exe {
                        ExeScriptCommand::Deploy { .. } => {
                            send_state(
                                &report_url,
                                &activity_id,
                                ActivityState::from(StatePair(State::Deployed, None)),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            log::info!(
                                "Got deploy command, changing state of exe unit to deployed",
                            );
                            result.push(ExeScriptCommandResult {
                                index: result.len() as u32,
                                result: CommandResult::Ok,
                                stdout: None,
                                stderr: None,
                                message: None,
                                is_batch_finished: false,
                                event_date: Utc::now(),
                            });
                        }
                        ExeScriptCommand::Start { args, .. } => {
                            let args = process::RuntimeArgs::new(args).map_err(|e| {
                                RpcMessageError::Activity(format!("invalid args: {}", e))
                            })?;

                            send_state(
                                &report_url,
                                &activity_id,
                                ActivityState::from(StatePair(State::Deployed, Some(State::Ready))),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            process_controller
                                .start(&args)
                                .await
                                .map_err(|e| RpcMessageError::Activity(e.to_string()))?;

                            send_state(
                                &report_url,
                                &activity_id,
                                ActivityState::from(StatePair(State::Ready, None)),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            log::info!("Got start command, changing state of exe unit to ready",);
                            result.push(ExeScriptCommandResult {
                                index: result.len() as u32,
                                result: CommandResult::Ok,
                                stdout: None,
                                stderr: None,
                                message: None,
                                is_batch_finished: false,
                                event_date: Utc::now(),
                            })
                        }
                        ExeScriptCommand::Terminate { .. } => {
                            process_controller.stop().await;
                            send_state(
                                &report_url,
                                &activity_id,
                                ActivityState::from(StatePair(State::Terminated, None)),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;
                            result.push(ExeScriptCommandResult {
                                index: result.len() as u32,
                                result: CommandResult::Ok,
                                stdout: None,
                                stderr: None,
                                message: None,
                                is_batch_finished: false,
                                event_date: Utc::now(),
                            });
                        }
                        cmd => {
                            return Err(RpcMessageError::Activity(format!(
                                "invalid command for mining exe-unit: {:?}",
                                cmd
                            )))
                        }
                    }
                }
                log::info!(
                    "got exec {}, batch_id={}, script={:?}",
                    exec.activity_id,
                    exec.batch_id,
                    exec.exe_script
                );

                {
                    let _ = batch.borrow_mut().insert(exec.batch_id.clone(), result);
                }

                Ok(exec.batch_id)
            }
        });

        gsb::bind(&exe_unit_url, move |exec: activity::GetExecBatchResults| {
            if let Some(result) = batch_results.borrow().get(&exec.batch_id) {
                future::ok(result.clone())
            } else {
                future::err(RpcMessageError::NotFound(exec.batch_id))
            }
        });
    };
    send_state(
        report_url,
        activity_id,
        ActivityState::from(StatePair(State::Initialized, None)),
    )
    .await?;

    activity_pinger.await?;
    log::info!("Finished waiting");

    Ok(())
}
