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
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Bind to Service Bus
    ServiceBus {
        /// ExeUnit service ID
        #[arg(long, short)]
        service_id: String,
        /// ExeUnit daemon GSB URL
        #[arg(long, short)]
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

#[cfg(test)]
mod test {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_args() {
        let cli = Cli::parse_from(["-b", "/tmp/false-runtime", "offer-template"]);
        assert_eq!(cli.binary, Some(Path::new("/tmp/false-runtime")));
        assert!(matches!(cli.command, Command::OfferTemplate));
    }
}
