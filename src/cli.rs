//! Exe-Unit Cli Definitions
//!

use clap::{Parser, Subcommand};
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    /// Runtime binary path
    #[arg(long, short)]
    pub binary: Option<PathBuf>,
    /// Runtime package name
    #[arg(long, short)]
    pub runtime: String,
    /// Runtime config. A text in json format or a path to a json file.
    #[arg(long,value_parser = parse_runtime_config)]
    pub runtime_config: Option<serde_json::Value>,
    #[command(subcommand)]
    pub command: Command,
}

fn parse_runtime_config(runtime_config: &str) -> anyhow::Result<serde_json::Value> {
    let config_file = Path::new(runtime_config);
    if config_file.exists() {
        let config_file = File::open(config_file)?;
        let reader = BufReader::new(config_file);
        return Ok(serde_json::from_reader(reader)?);
    }
    log::info!("Raw runtime config arg: {runtime_config}");
    Ok(serde_json::from_str(runtime_config)?)
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Bind to Service Bus
    ServiceBus {
        /// ExeUnit service ID
        service_id: String,
        /// ExeUnit daemon GSB URL
        report_url: String,
        #[command(flatten)]
        args: RunArgs,
    },
    /// Print an offer template in JSON format
    OfferTemplate,
    /// Run runtime's tests command
    Test,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Agreement file path
    #[arg(long, short)]
    pub agreement: PathBuf,
    /// Working directory
    #[arg(long, short)]
    pub work_dir: PathBuf,
    /// Common cache directory
    #[arg(long, short)]
    pub cache_dir: PathBuf,
}
