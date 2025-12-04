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

    /// Print a screenshot and exit (for testing)
    #[arg(long)]
    screenshot: bool,

    /// Show popup in screenshot (info, warning, error)
    #[arg(long, value_name = "LEVEL")]
    popup: Option<String>,

    /// Show busy spinner in screenshot
    #[arg(long, value_name = "MESSAGE")]
    busy: Option<String>,

    /// Simulate loaded file in screenshot
    #[arg(long, value_name = "FILENAME")]
    file: Option<String>,

    /// Set active tab in screenshot (1-4 or name)
    #[arg(long, value_name = "TAB")]
    tab: Option<String>,
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

    let mut app = App::new()?;

    // Screenshot mode - render one frame and exit
    if args.screenshot {
        if let Some(level) = args.popup {
            match level.as_str() {
                "info" => app.show_info("Info", "This is an info message."),
                "warning" => app.show_warning("Warning", "This is a warning message."),
                "error" => app.show_error("Error", "This is an error message."),
                _ => app.show_info("Unknown", &format!("Unknown level: {}", level)),
            }
        }
        if let Some(busy_msg) = args.busy {
            app.set_busy(busy_msg);
        }
        if let Some(filename) = args.file {
            app.set_loaded_file(std::path::PathBuf::from(filename));
        }
        if let Some(tab) = args.tab {
            app.set_tab(&tab);
        }
        println!("{}", app.screenshot(80, 20));
        return Ok(());
    }

    // Run the TUI application
    app.run()?;

    Ok(())
}
