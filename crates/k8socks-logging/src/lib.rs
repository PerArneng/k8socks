use colored::*;
use std::str::FromStr;
use tracing::Level;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::{FmtContext, Layer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// A custom event formatter that produces logs in the desired format.
struct CustomFormatter {
    use_color: bool,
}

impl<S, N> FormatEvent<S, N> for CustomFormatter
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    N: for<'a> tracing_subscriber::fmt::FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let level = *event.metadata().level();
        let time = chrono::Local::now().format("%Y-%m-%d %H:%M");

        let level_str = if self.use_color {
            match level {
                Level::TRACE => "TRACE".magenta().bold(),
                Level::DEBUG => "DEBUG".blue().bold(),
                Level::INFO => " INFO".green().bold(),
                Level::WARN => " WARN".yellow().bold(),
                Level::ERROR => "ERROR".red().bold(),
            }
        } else {
            match level {
                Level::TRACE => "TRACE".normal(),
                Level::DEBUG => "DEBUG".normal(),
                Level::INFO => " INFO".normal(),
                Level::WARN => " WARN".normal(),
                Level::ERROR => "ERROR".normal(),
            }
        };

        write!(writer, "{} {} ", time, level_str)?;

        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

/// Initializes the global logger.
pub fn init_logging(
    level_str: &str,
    use_color: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let level = Level::from_str(level_str).unwrap_or(Level::INFO);

    let env_filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    let formatter = CustomFormatter { use_color };

    let layer = Layer::default().event_format(formatter);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(layer)
        .init();

    Ok(())
}