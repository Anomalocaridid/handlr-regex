use notify_rust::{Notification, Timeout, Urgency};
use tracing::{field::Visit, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter, fmt, layer::SubscriberExt, Layer};

use crate::{cli::Cli, error::Result};

/// Init global tracing subscriber
pub fn init_tracing(cli: &Cli) -> Result<WorkerGuard> {
    let (file_writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::never(
            xdg::BaseDirectories::new()?.create_cache_directory("handlr")?,
            "handlr.log",
        ));

    // Filter logs based on `$RUST_LOG` and cli args
    let env_filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|var| var.parse::<filter::Targets>().ok())
        .unwrap_or_else(|| filter::Targets::new().with_default(cli.verbosity));

    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            // Send filtered logs to stdout
            .with(
                fmt::Layer::new()
                    .with_writer(std::io::stderr)
                    .with_filter(env_filter.clone()),
            )
            // Send all logs to a log file
            .with(fmt::Layer::new().with_writer(file_writer).with_ansi(false))
            // Notify for filtered logs
            .with(
                cli.show_notifications()
                    .then_some(NotificationLayer.with_filter(env_filter)),
            )
            // Never any logs from other crates so the user is not overwhelmed by the output
            .with(
                filter::Targets::new()
                    .with_target("handlr", Level::TRACE)
                    .with_target("tracing_unwrap", Level::WARN),
            ),
    )?;

    Ok(guard)
}

/// Custom tracing layer for running a notification on relevant events
struct NotificationLayer;

impl<S> Layer<S> for NotificationLayer
where
    S: tracing::Subscriber,
{
    #[mutants::skip] // Cannot test, relies on dbus
    fn on_event(
        &self,
        event: &tracing::Event,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut message = String::new();
        event.record(&mut NotificationVisitor(&mut message));

        // Just in case a message has no message, but some other information
        if message.is_empty() {
            message = "No message, see ~/.cache/handlr/handlr.log for details"
                .to_string()
        }

        let (level, icon, urgency) = match *event.metadata().level() {
            tracing::Level::ERROR => {
                ("error".to_string(), "dialog-error", Urgency::Critical)
            }
            tracing::Level::WARN => {
                ("warning".to_string(), "dialog-warning", Urgency::Normal)
            }
            l => (
                l.as_str().to_lowercase(),
                "dialog-information",
                Urgency::Low,
            ),
        };

        Notification::new()
            .summary(&format!("handlr {}", level))
            .body(&message)
            .icon(icon)
            .appname("handlr")
            .timeout(Timeout::Milliseconds(10_000))
            .urgency(urgency)
            .show()
            .expect("handlr error: Could not issue dbus notification");
    }
}

struct NotificationVisitor<'a>(&'a mut String);

impl Visit for NotificationVisitor<'_> {
    #[mutants::skip] // Cannot test independently of NotificationLayer
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        if field.name() == "message" {
            self.0.push_str(&format!("{:?}", value));
        }
    }
}
