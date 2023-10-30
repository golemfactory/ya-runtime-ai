//! Exe-Unit Cli Definitions
//!

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    /// Runtime binary path
    #[arg(long, short)]
    pub binary: Option<PathBuf>,
    /// Runtime package name
    #[arg(long, short)]
    pub runtime: String,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /*
    /// Execute commands from file
    FromFile {
        /// ExeUnit daemon GSB URL
        #[arg(long)]
        report_url: Option<String>,
        /// ExeUnit service ID
        #[arg(long)]
        service_id: Option<String>,
        /// Command file path
        input: PathBuf,
        #[command(flatten)]
        args: RunArgs,
    },
     */
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
    /// Run runtime's test command
    Test,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Agreement file path
    #[arg(long, short)]
    pub agreement: PathBuf,
    /// Working directory
    #[arg(long, short)]
    pub work_dir: Option<PathBuf>,
    /// Common cache directory
    #[arg(long, short)]
    pub cache_dir: Option<PathBuf>,
}
