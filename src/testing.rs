//! Testing helpers
#![cfg(test)]

/// Helper function for insta settings
pub fn timestamp_filter() -> Vec<(&'static str, &'static str)> {
    vec![(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d*\.\d*Z", "[TIMESTAMP]")]
}

/// Helper macro to snapshot test logs
#[macro_export]
macro_rules! logs_snapshot_test {
    ($name:ident, $block:block) => {
        #[test]
        fn $name() -> $crate::error::Result<()> {
            use std::io::Read;

            use tracing::{subscriber, Level};
            use tracing_subscriber::{
                filter, fmt, layer::SubscriberExt, registry,
            };

            let (mut pipe_read, pipe_write) = pipe::pipe();

            // Create scope so guards are dropped before assertion and logs are flushed
            // Otherwise, the tests using this macro will freeze
            {
                let (writer, _writer_guard) =
                    tracing_appender::non_blocking(pipe_write);
                let _default_guard = subscriber::set_default(
                    registry()
                        .with(fmt::Layer::new().with_writer(writer))
                        // Filter out all logs from other crates so the user is not overwhelmed looking at the logs
                        .with(
                            filter::Targets::new()
                                .with_target("handlr", Level::TRACE)
                                .with_target("tracing_unwrap", Level::WARN),
                        ),
                );
                $block
            }

            let mut buffer = Vec::<u8>::new();
            pipe_read.read_to_end(&mut buffer)?;

            insta::with_settings!(
                {
                    filters => $crate::testing::timestamp_filter()
                },
                { insta::assert_snapshot!(String::from_utf8(buffer).expect("Buffer is invalid utf8")) }
            );

            Ok(())
        }
    };
}
