use std::time::Duration;

use serde::Deserialize;

use crate::process::RuntimeConfig;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub(crate) struct Config {
    pub startup_script: String,

    pub api_port: u16,

    pub api_host: String,

    pub api_shutdown_path: String,

    pub model_arg: String,

    pub additional_args: Vec<String>,

    // Monitor
    #[serde(with = "humantime_serde")]
    pub startup_timeout: Duration,

    #[serde(with = "humantime_serde")]
    pub api_ping_delay: Duration,

    pub monitored_startup_msg: String,

    pub monitored_model_failure_msg: String,

    pub monitored_msgs_w_trace_lvl: Vec<String>,

    pub gpu_uuid: Option<String>,
}

impl RuntimeConfig for Config {
    fn gpu_uuid(&self) -> Option<String> {
        self.gpu_uuid.clone()
    }

    fn uses_gpu() -> bool {
        true
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            startup_script: "sd.webui_noxformers/run.bat".into(),
            api_port: 7861,
            api_host: "localhost".into(),
            api_shutdown_path: "sdapi/v1/server-kill".into(),
            model_arg: "--ckpt".into(),
            additional_args: vec![
                "--skip-torch-cuda-test".into(),
                "--skip-python-version-check".into(),
                "--skip-version-check".into(),
            ],
            startup_timeout: Duration::from_secs(90),
            api_ping_delay: Duration::from_millis(997),
            monitored_startup_msg: "Model loaded in ".into(),
            monitored_model_failure_msg: "Stable diffusion model failed to load".into(),
            monitored_msgs_w_trace_lvl: vec![
                // log generated by API ping task
                "\"GET / HTTP/1.1\" 404 Not Found".into(),
            ],
            gpu_uuid: None,
        }
    }
}
