use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use actix::prelude::*;
use anyhow::Context;
use chrono::Utc;
use clap::Parser;
use futures::prelude::*;
use ya_gsb_http_proxy::gsb_to_http::GsbToHttpProxy;
use ya_gsb_http_proxy::message::GsbHttpCallMessage;

use process::Runtime;
use tokio::select;
use tokio::sync::{mpsc, mpsc::Receiver, mpsc::Sender};
use tokio_util::sync::PollSender;
use ya_client_model::activity::activity_state::*;
use ya_client_model::activity::{ActivityUsage, CommandProgress, ExeScriptCommand};
use ya_core_model::activity;
use ya_core_model::activity::RpcMessageError;
use ya_service_bus::typed as gsb;
use ya_transfer::transfer::{DeployImage, Shutdown, TransferService, TransferServiceContext};

use crate::agreement::AgreementDesc;
use crate::batches::Batches;
use crate::cli::*;
use crate::logger::*;
use crate::process::ProcessController;
use crate::signal::SignalMonitor;

mod agreement;
mod batches;
mod cli;
mod logger;
mod offer_template;
mod process;
mod signal;

pub type Signal = &'static str;

async fn send_state<T: process::Runtime>(
    ctx: &ExeUnitContext<T>,
    new_state: ActivityState,
) -> anyhow::Result<()> {
    Ok(gsb::service(ctx.report_url.clone())
        .call(activity::local::SetState::new(
            ctx.activity_id.clone(),
            new_state,
            None,
        ))
        .await??)
}

async fn activity_loop<T: process::Runtime + Clone + Unpin + 'static>(
    report_url: &str,
    activity_id: &str,
    process: ProcessController<T>,
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

        select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            status = process.clone() => {
                if let Err(err) = report_service.call(activity::local::SetState {
                    activity_id: activity_id.to_string(),
                    state: ActivityState {
                        state: StatePair::from(State::Terminated),
                        reason: Some("process exit".to_string()),
                        error_message: Some(format!("status: {:?}", status)),
                    },
                    timeout: None,
                    credentials: None,
                }).await {
                    log::error!("Failed to send state. Err {err}");
                }
                log::error!("process exit: {:?}", status);
                anyhow::bail!("Runtime exited");
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
    log::debug!("Raw CLI args: {:?}", std::env::args_os());
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            log::error!("Failed to parse CLI: {}", err);
            err.exit();
        }
    };

    let (signal_sender, signal_receiver) = mpsc::channel::<Signal>(1);

    select! {
        res = handle_cli(cli, signal_receiver) => res,
        res = handle_signals(signal_sender) => res,
    }
}

async fn handle_cli(cli: Cli, signal_receiver: Receiver<Signal>) -> anyhow::Result<()> {
    match cli.runtime.to_lowercase().as_str() {
        "dummy" => run::<process::dummy::Dummy>(cli, signal_receiver).await,
        "automatic" => run::<process::automatic::Automatic>(cli, signal_receiver).await,
        _ => {
            let err = anyhow::format_err!("Unsupported framework {}", cli.runtime);
            log::error!("{}", err);
            anyhow::bail!(err)
        }
    }
}

async fn handle_signals(signal_receiver: Sender<Signal>) -> anyhow::Result<()> {
    let signal = SignalMonitor::default().recv().await?;
    log::info!("{} received, Shutting down runtime...", signal);
    Ok(signal_receiver.send(signal).await?)
}

#[derive(Clone)]
struct ExeUnitContext<T: Runtime + 'static> {
    pub activity_id: String,
    pub report_url: String,

    pub agreement: AgreementDesc,
    pub transfers: Addr<TransferService>,
    pub process_controller: ProcessController<T>,

    pub batches: Batches,

    pub model_path: Option<PathBuf>,
}

async fn run<RUNTIME: process::Runtime + Clone + Unpin + 'static>(
    cli: Cli,
    mut signal_receiver: Receiver<Signal>,
) -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let runtime_config = Box::pin(RUNTIME::parse_config(&cli.runtime_config)?);
    log::info!("Runtime config: {runtime_config:?}");

    let (exe_unit_url, report_url, activity_id, args) = match &cli.command {
        Command::ServiceBus {
            service_id,
            report_url,
            args,
            ..
        } => (
            ya_core_model::activity::exeunit::bus_id(service_id),
            report_url,
            service_id,
            args,
        ),
        Command::OfferTemplate => {
            let template = offer_template::template()?;
            io::stdout().write_all(template.as_ref())?;
            return Ok(());
        }
        Command::Test => {
            // Test
            return Ok(());
        }
    };

    let agreement_path = args.agreement.clone();

    let agreement = AgreementDesc::load(agreement_path)?;

    let ctx = ExeUnitContext {
        activity_id: activity_id.clone(),
        report_url: report_url.clone(),
        agreement,
        transfers: TransferService::new(TransferServiceContext {
            work_dir: args.work_dir.clone(),
            cache_dir: args.cache_dir.clone(),
            ..TransferServiceContext::default()
        })
        .start(),
        process_controller: process::ProcessController::<RUNTIME>::new(),
        batches: Batches::default(),
        model_path: None,
    };

    let activity_pinger = activity_loop(
        report_url,
        activity_id,
        ctx.process_controller.clone(),
        ctx.agreement.clone(),
    );

    #[cfg(target_os = "windows")]
    let _job = process::win::JobObject::new()?;
    {
        let batch = ctx.batches.clone();
        batch.bind_gsb(&exe_unit_url);

        let ctx = ctx.clone();
        gsb::bind(&exe_unit_url, move |exec: activity::Exec| {
            let exec = exec.clone();
            let batch_id = exec.batch_id.clone();
            let runtime_config = runtime_config.clone();

            let batch = ctx.batches.start_batch(&exec.batch_id);
            let batch_ = batch.clone();
            let mut ctx = ctx.clone();
            let script_future = async move {
                for exe in &exec.exe_script {
                    match exe {
                        cmd @ ExeScriptCommand::Deploy { progress, .. } => {
                            let index = batch.next_command(cmd);
                            send_state(
                                &ctx,
                                ActivityState::from(StatePair(
                                    State::Initialized,
                                    Some(State::Deployed),
                                )),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            log::info!(
                                "Got Deploy command. Deploying image: {}",
                                ctx.agreement.model
                            );

                            let (tx, mut rx) = mpsc::channel::<CommandProgress>(1);

                            let batch_ = batch.clone();
                            tokio::task::spawn_local(async move {
                                while let Some(progress) = rx.recv().await {
                                    let percent = 100.0 * progress.progress.0 as f64
                                        / progress.progress.1.unwrap_or(1) as f64;

                                    log::info!(
                                        "Deploy progress: {percent}% ({}/{})",
                                        progress.progress.0,
                                        progress.progress.1.unwrap_or(0)
                                    );
                                    batch_.update_progress(index, &progress);
                                }
                            });

                            let mut deploy = DeployImage::with_package(&ctx.agreement.model);
                            if let Some(args) = progress {
                                deploy.forward_progress(
                                    args,
                                    PollSender::new(tx).sink_map_err(|e| {
                                        ya_transfer::error::Error::Other(e.to_string())
                                    }),
                                )
                            }

                            ctx.model_path = ctx
                                .transfers
                                .send(deploy)
                                .await
                                .map_err(|e| format!("Failed to send DeployImage: {e}"))
                                .map_err(RpcMessageError::Service)?
                                .map_err(|e| format!("DeployImage failed: {e}"))
                                .map_err(RpcMessageError::Service)?;

                            log::info!("Image deployed: {}", ctx.agreement.model);

                            send_state(&ctx, ActivityState::from(StatePair(State::Deployed, None)))
                                .await
                                .map_err(|e| RpcMessageError::Service(e.to_string()))?;
                            batch.ok_result();
                        }
                        cmd @ ExeScriptCommand::Start { args, .. } => {
                            log::debug!("Raw Start cmd args: {args:?} [ignored]");

                            batch.next_command(cmd);
                            send_state(
                                &ctx,
                                ActivityState::from(StatePair(State::Deployed, Some(State::Ready))),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            ctx.process_controller
                                .start(ctx.model_path.clone(), (*runtime_config).clone())
                                .await
                                .map_err(|e| RpcMessageError::Activity(e.to_string()))?;
                            log::debug!("Started process");

                            send_state(&ctx, ActivityState::from(StatePair(State::Ready, None)))
                                .await
                                .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            log::info!("Got start command, changing state of exe unit to ready",);
                            batch.ok_result();
                        }
                        cmd @ ExeScriptCommand::Terminate { .. } => {
                            batch.next_command(cmd);

                            log::info!("Raw Terminate command. Stopping runtime",);
                            if let Err(err) = ctx.process_controller.stop().await {
                                log::error!("Failed to terminate process. Err {err}");
                            }
                            ctx.transfers.send(Shutdown {}).await.ok();
                            send_state(
                                &ctx,
                                ActivityState::from(StatePair(State::Terminated, None)),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            batch.ok_result();
                        }
                        cmd => {
                            return Err(RpcMessageError::Activity(format!(
                                "invalid command for ai runtime: {cmd:?}",
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

                batch.finish();
                Ok(exec.batch_id)
            }
            .map_err(move |e| {
                log::error!("ExeScript failure: {e:?}");
                batch_.err_result(Some(e.to_string()));
                batch_.finish();
            });
            tokio::task::spawn_local(script_future);
            future::ok(batch_id)
        });

        gsb::bind_stream(&exe_unit_url, move |message: GsbHttpCallMessage| {
            let mut proxy = GsbToHttpProxy {
                // base_url: "http://10.30.13.8:7861/".to_string(),
                base_url: "http://localhost:7861/".to_string(),
            };
            let stream = proxy.pass(message);
            Box::pin(stream.map(Ok))
        });
    };
    send_state(
        &ctx,
        ActivityState::from(StatePair(State::Initialized, None)),
    )
    .await?;

    select! {
        res = activity_pinger => { res }
        signal = signal_receiver.recv() => {
            if let Some(signal) = signal {
                log::debug!("Received signal {signal}. Stopping runtime");

                ctx.process_controller.stop().await
                    .context("Stopping runtime error")?;
            }
            Ok(())
        },
    }
    .context("Activity loop error")?;

    log::info!("Finished waiting");
    send_state(
        &ctx,
        ActivityState::from(StatePair(State::Terminated, None)),
    )
    .await?;

    Ok(())
}
