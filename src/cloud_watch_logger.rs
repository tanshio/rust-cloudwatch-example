use again::RetryPolicy;
use chrono::Utc;

use log::Level;
use pretty_env_logger::env_logger::fmt::{Color, Style, StyledValue};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, DescribeLogStreamsRequest, InputLogEvent,
    PutLogEventsRequest,
};
use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);

const LOG_GROUP_NAME: &str = "test-group";
const LOG_STREAM_NAME: &str = "test-stream";

fn max_target_width(target: &str) -> usize {
    let max_width = MAX_MODULE_WIDTH.load(Ordering::Relaxed);
    if max_width < target.len() {
        MAX_MODULE_WIDTH.store(target.len(), Ordering::Relaxed);
        target.len()
    } else {
        max_width
    }
}

fn colored_level<'a>(style: &'a mut Style, level: Level) -> StyledValue<'a, &'static str> {
    match level {
        Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        Level::Info => style.set_color(Color::Green).value("INFO "),
        Level::Warn => style.set_color(Color::Yellow).value("WARN "),
        Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
}

struct Padded<T> {
    value: T,
    width: usize,
}

impl<T: fmt::Display> fmt::Display for Padded<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{: <width$}", self.value, width = self.width)
    }
}

pub struct CloudWatchLogger {
    client: CloudWatchLogsClient,
}

async fn send(client: CloudWatchLogsClient, message: String) {
    let timestamp = Utc::now().timestamp_millis();

    let policy = RetryPolicy::exponential(Duration::from_secs(1))
        .with_jitter(false)
        .with_max_delay(Duration::from_secs(60))
        .with_max_retries(4);

    policy
        .retry(|| async {
            let mut desc_streams_req: DescribeLogStreamsRequest = Default::default();
            desc_streams_req.log_group_name = LOG_GROUP_NAME.to_string();

            let p = RetryPolicy::exponential(Duration::from_secs(1))
                .with_jitter(false)
                .with_max_delay(Duration::from_secs(60))
                .with_max_retries(10);

            let log_streams = p
                .retry(|| async {
                    let streams_resp = client.describe_log_streams(desc_streams_req.clone()).await;
                    return if streams_resp.is_err() {
                        Err("throttle error")
                    } else {
                        Ok(streams_resp.unwrap())
                    };
                })
                .await;

            let log_streams = log_streams.unwrap().log_streams.unwrap();

            let stream = &log_streams
                .iter()
                .find(|s| s.log_stream_name == Some(LOG_STREAM_NAME.to_string()))
                .unwrap();
            let sequence_token = stream.upload_sequence_token.clone();
            let input_log_event = InputLogEvent {
                message: message.clone(),
                timestamp,
            };
            let put_log_events_request = PutLogEventsRequest {
                log_events: vec![input_log_event],
                log_group_name: LOG_GROUP_NAME.to_string(),
                log_stream_name: LOG_STREAM_NAME.to_string(),
                sequence_token: sequence_token.clone(),
            };
            client.put_log_events(put_log_events_request).await
        })
        .await
        .expect("should send log");
}

impl CloudWatchLogger {
    pub fn new(client: CloudWatchLogsClient) -> Self {
        Self { client }
    }
    pub fn build(&self) -> pretty_env_logger::env_logger::Logger {
        let client = self.client.clone();
        let mut logger = pretty_env_logger::formatted_builder();

        let builder = logger.format(move |f, record| {
            let target = record.target();
            let max_width = max_target_width(target);
            let mut style = f.style();
            let level = colored_level(&mut style, record.level());
            let mut style = f.style();
            let target = style.set_bold(true).value(Padded {
                value: target,
                width: max_width,
            });
            let message = format!(" {} {} > {}", level, target, record.args());
            tokio::spawn(send(client.clone(), message));

            writeln!(f, " {} {} > {}", level, target, record.args(),)
        });

        if let Ok(s) = std::env::var("RUST_LOG") {
            builder.parse_filters(&s);
        }

        builder.build()
    }
}
