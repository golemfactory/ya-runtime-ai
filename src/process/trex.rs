use std::env;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::Duration;

use super::{MiningAppArgs, Shares, ENV_EXTRA_PARAMS};

#[derive(Serialize, Deserialize)]
struct TRexMinerDataActivePool {
    difficulty: String, //"difficulty":"4.00 G",
}

#[derive(Serialize, Deserialize)]
struct TRexMinerData {
    accepted_count: u64,
    invalid_count: u64,
    rejected_count: u64,
    active_pool: TRexMinerDataActivePool,
    hashrate: u64,
    hashrate_hour: u64,
    hashrate_minute: u64,
}

async fn get_trex_info() -> anyhow::Result<TRexMinerData> {
    let content = reqwest::get("http://localhost:4067/summary")
        .await?
        .text_with_charset("utf-8")
        .await?;

    let trex_miner_data = serde_json::from_str::<TRexMinerData>(&content)?;

    Ok(trex_miner_data)
}

#[derive(Clone)]
pub struct Trex {}

impl super::MinerEngine for Trex {
    fn start(args: &MiningAppArgs) -> anyhow::Result<Child> {
        let exe = super::find_exe("t-rex\\t-rex.exe")?;
        let _cmd = Command::new(&exe);
        let work_dir = exe.parent().unwrap();

        let mut cmd = Command::new(&exe);
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir)
            .arg("--no-watchdog")
            .arg("-o")
            .arg(&args.pool)
            .arg("-u")
            .arg(&args.wallet);

        if let Some(ref worker_name) = args.worker_name {
            cmd.arg("-w").arg(worker_name);
        }

        if let Ok(param) = env::var(ENV_EXTRA_PARAMS) {
            cmd.args(param.split(' '));
        }

        Ok(cmd.kill_on_drop(true).spawn()?)
    }

    fn run<ReportFn: Fn(Shares) + 'static>(stdout: ChildStdout, report_fn: ReportFn) {
        tokio::task::spawn_local(async move {
            let mut stdout = BufReader::new(stdout);
            let mut line_buf = String::new();
            loop {
                match stdout.read_line(&mut line_buf).await {
                    Err(e) => {
                        log::error!("no line: {}", e);
                        break;
                    }
                    Ok(0) => break,
                    Ok(_) => (),
                }
                let line = line_buf.trim_end();
                log::info!("trex: {}", line);
            }
        });
        tokio::task::spawn_local(async move {
            let mut prev_shares: u64 = 0;
            let mut prev_stale_shares: u64 = 0;
            let mut prev_invalid_shares: u64 = 0;
            let mut difficulty: f64;
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let trex_miner_data = match get_trex_info().await {
                    Ok(t) => t,
                    Err(e) => {
                        log::error!("Failed to parse rexMinerData {}", e);
                        break;
                    }
                };
                let speed: f64 = trex_miner_data.hashrate_minute as f64 / 1000000.0;
                let shares: u64 = trex_miner_data.accepted_count;
                let stale_shares: u64 = trex_miner_data.rejected_count;
                let invalid_shares: u64 = trex_miner_data.invalid_count;
                let difficulty_str = trex_miner_data.active_pool.difficulty.replace(" G", "");
                difficulty = match difficulty_str.parse::<f64>() {
                    Ok(val) => val * 1000.0, //convert to Mh
                    Err(e) => {
                        log::error!("Failed to parse difficulty {}", e);
                        break;
                    }
                };
                if shares >= prev_shares
                    || stale_shares >= prev_stale_shares
                    || invalid_shares >= prev_invalid_shares
                {
                    report_fn(Shares {
                        cnt: shares - prev_shares,
                        stale_cnt: stale_shares - prev_stale_shares,
                        invalid_cnt: invalid_shares - prev_invalid_shares,
                        new_speed: speed,
                        difficulty,
                    });
                    prev_shares = shares;
                    prev_stale_shares = stale_shares;
                    prev_invalid_shares = invalid_shares;
                } else {
                    log::error!(
                        "invalid shares decoded: ({},{},{})/{}",
                        shares,
                        stale_shares,
                        invalid_shares,
                        prev_shares
                    );
                }
            }
        });
    }
}
