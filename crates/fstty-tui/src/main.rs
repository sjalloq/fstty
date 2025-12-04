//! fstty - A TUI for waveform analysis
//!
//! Pronounced "fiesty"

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use fstty_tui::App;

/// fstty - A TUI for waveform analysis
#[derive(Parser, Debug)]
#[command(name = "fstty")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable debug logging to file
    #[arg(short, long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Set up logging
    if args.debug {
        let file_appender = tracing_appender::rolling::never(".", "fstty.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false),
            )
            .init();
    }

    // Run the TUI application
    let mut app = App::new();
    app.run()?;

    Ok(())
}
