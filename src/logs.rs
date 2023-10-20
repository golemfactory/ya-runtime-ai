

pub fn colored_stderr_exeunit_prefixed_format(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), std::io::Error> {
    write!(w, "{}", yansi::Color::Fixed(92).paint("[ExeUnit] "))?;
    flexi_logger::colored_opt_format(w, now, record)
}

fn configure_logger(logger: flexi_logger::Logger) -> flexi_logger::Logger {
    logger
        .format(flexi_logger::colored_opt_format)
        .duplicate_to_stderr(flexi_logger::Duplicate::Debug)
        .format_for_stderr(colored_stderr_exeunit_prefixed_format)
}


pub fn init() -> anyhow::Result<()> {
    let default_log_level = "debug";
    if configure_logger(flexi_logger::Logger::with_env_or_str(default_log_level))
        .log_to_file()
        .directory("logs")
        .start()
        .is_err()
    {
        configure_logger(flexi_logger::Logger::with_env_or_str(default_log_level))
            .start()
            .expect("Failed to initialize logging");
        log::warn!("Switched to fallback logging method");
    }
    Ok(())
}
