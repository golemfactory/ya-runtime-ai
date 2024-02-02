use assert_cmd::prelude::*; // Add methods on commands
use std::{path::PathBuf, process::Command}; // Run programs

#[test]
fn runtime_config_as_text_ok() {
    let mut cmd = Command::cargo_bin("ya-runtime-ai").unwrap();
    cmd.arg("--runtime")
        .arg("automatic")
        .arg("--runtime-config")
        .arg(
            "{ \
            \"startup_script\": \"path/run.bat\", \
            \"api_port\": 80, \
            \"api_host\": \"domain.com\", \
            \"api_shutdown_path\": \"/kill/me\", \
            \"model_arg\": \"\", \
            \"additional_args\": [\"--arg-one\", \"--arg-two\"], \
            \"startup_timeout\": \"1s\", \
            \"api_ping_delay\": \"100ms\", \
            \"monitored_startup_msg\": \"Started\", \
            \"monitored_model_failure_msg\": \"Failed\", \
            \"monitored_msgs_w_trace_lvl\": [\"Unimportant\", \"Boring log\"] \
        }",
        )
        .arg("test")
        .assert()
        .success();
}

#[test]
fn config_parse_succ_single_field() {
    let mut cmd = Command::cargo_bin("ya-runtime-ai").unwrap();
    cmd.arg("--runtime")
        .arg("automatic")
        .arg("--runtime-config")
        .arg("{ \"startup_script\": \"path/bin.exe\" }")
        .arg("test")
        .assert()
        .success();
}

#[test]
fn config_parse_fail_field_bat_type() {
    let mut cmd = Command::cargo_bin("ya-runtime-ai").unwrap();
    cmd.arg("--runtime")
        .arg("automatic")
        .arg("--runtime-config")
        .arg("{ \"startup_script\": 13 }")
        .arg("test")
        .assert()
        .failure();
}

#[test]
fn succ_without_runtime_config_arg() {
    let mut cmd = Command::cargo_bin("ya-runtime-ai").unwrap();
    cmd.arg("--runtime")
        .arg("dummy")
        .arg("test")
        .assert()
        .success();
}

#[test]
fn config_parse_file_succ() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/resources/runtime_config.json");
    let mut cmd = Command::cargo_bin("ya-runtime-ai").unwrap();
    cmd.arg("--runtime")
        .arg("automatic")
        .arg("--runtime-config")
        .arg(path.to_str().unwrap())
        .arg("test")
        .assert()
        .success();
}
