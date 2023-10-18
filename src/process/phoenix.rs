use std::env;
use std::process::Stdio;

use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};

use super::{MinerEngine, MiningAppArgs, Shares, ENV_EXTRA_PARAMS};

#[derive(Clone)]
pub struct Phoenix {}

impl Unpin for Phoenix {}

impl MinerEngine for Phoenix {
    fn start(args: &MiningAppArgs) -> anyhow::Result<Child> {
        let exe = super::find_exe("phoenix\\EthDcrMiner64.exe")?;
        let mut cmd = Command::new(&exe);
        let work_dir = exe.parent().unwrap();
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::null())
            .current_dir(work_dir)
            .arg("-epool")
            .arg(&args.pool)
            .arg("-ewal")
            .arg(&args.wallet);

        if let Some(ref worker_name) = args.worker_name {
            cmd.arg("-eworker").arg(worker_name);
        }

        if let Some(ref pw) = args.password {
            cmd.arg("-epsw").arg(pw);
        }

        if let Ok(param) = env::var(ENV_EXTRA_PARAMS) {
            cmd.args(param.split(' '));
        }

        Ok(cmd.kill_on_drop(true).spawn()?)
    }

    fn run<ReportFn: Fn(Shares) + 'static>(stdout: ChildStdout, report_fn: ReportFn) {
        tokio::task::spawn_local(async move {
            let re = Regex::new(RE_PHOENIX_MATCH_ETH).unwrap();
            let re_new_job = Regex::new(RE_PHOENIX_MATCH_NEW_JOB).unwrap();
            let mut stdout = BufReader::new(stdout);
            let mut prev_shares: u64 = 0;
            let mut prev_stale_shares: u64 = 0;
            let mut prev_invalid_shares: u64 = 0;
            let mut difficulty: f64 = 0.0;
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
                if let Some(it) = re.captures(&line) {
                    log::info!("matched: {}", line);
                    match (|| -> anyhow::Result<(u64, u64, u64, f64)> {
                        let speed: f64 = it[1].parse()?;
                        let shares: u64 = it[2].parse()?;
                        let stale_shares: u64 = it[3].parse()?;
                        let invalid_shares: u64 = it[4].parse()?;
                        Ok((shares, stale_shares, invalid_shares, speed))
                    })() {
                        Ok((shares, stale_shares, invalid_shares, speed)) => {
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
                        Err(e) => log::error!("failed to parse line: {}", e),
                    }
                }
                if let Some(it) = re_new_job.captures(&line) {
                    let difficulty_str = &it[1];
                    difficulty = match difficulty_str.parse() {
                        Ok(v) => v,
                        Err(e) => {
                            log::error!("invalid dificulty: {}", e);
                            break;
                        }
                    }
                } else {
                    log::info!("stdout: {}", line);
                }
            }
        });
    }
}

const RE_PHOENIX_MATCH_ETH: &str =
    r"^Eth speed: ([0-9]*\.[0-9]*) MH/s, shares: ([0-9]+)/([0-9]+)/([0-9]+), time: ";

//
const RE_PHOENIX_MATCH_NEW_JOB: &str = r"^Eth: New job .*; diff: ([0-9]+(.[0-9]+)?)MH";

#[cfg(test)]
mod test {
    use regex::Regex;
    use structopt::StructOpt;

    use super::*;

    #[test]
    fn parse_output() {
        let re = Regex::new(RE_PHOENIX_MATCH_ETH).unwrap();
        let line = "Eth speed: 25.355 MH/s, shares: 5/0/0, time: 1:00\r\n";
        for it in re.captures_iter(line) {
            println!("[{}] {} - {} - {}", &it[1], &it[2], &it[3], &it[4]);
        }
    }

    #[test]
    fn parse_new_job() {
        let re = Regex::new(RE_PHOENIX_MATCH_NEW_JOB).unwrap();
        let line =
            "Eth: New job #00ADFC28 from staging-backend.chessongolem.app:3334; diff: 4295MH";
        let cap = re.captures(line).unwrap();
        assert_eq!(&cap[1], "4295");
    }

    #[test]
    fn parse_args() {
        assert!(MiningAppArgs::from_iter_safe(&["self"]).is_err());
        assert_eq!(
            MiningAppArgs::from_iter_safe(&[
                "self",
                "--pool",
                "staging-backend.chessongolem.app:3334",
                "--wallet",
                "0xa1a7c282badfa6bd188a6d42b5bc7fa1e836d1f8"
            ])
            .unwrap(),
            MiningAppArgs {
                pool: "staging-backend.chessongolem.app:3334".to_string(),
                wallet: "0xa1a7c282badfa6bd188a6d42b5bc7fa1e836d1f8".to_string(),
                worker_name: None,
                password: None
            }
        );
        assert_eq!(MiningAppArgs::from_iter_safe(&[
            "self",
            "--pool", "staging-backend.chessongolem.app:3334",
            "--worker-name", "0x246337ff755379bdbfd4381e35937526c150e541/0x34f24b55755de03984b55d1c650092c5e34d610d/82398862214e4c09854eededf2ad37c4",
            "--wallet", "0xa1a7c282badfa6bd188a6d42b5bc7fa1e836d1f8"]).unwrap(),
                   MiningAppArgs {
                       pool: "staging-backend.chessongolem.app:3334".to_string(),
                       wallet: "0xa1a7c282badfa6bd188a6d42b5bc7fa1e836d1f8".to_string(),
                       worker_name: Some("0x246337ff755379bdbfd4381e35937526c150e541/0x34f24b55755de03984b55d1c650092c5e34d610d/82398862214e4c09854eededf2ad37c4".into()),
                       password: None
                   });
    }
}
