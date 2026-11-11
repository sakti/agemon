use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use miette::{IntoDiagnostic, Result, miette};
use prometheus_remote_write::{
    CONTENT_TYPE, HEADER_NAME_REMOTE_WRITE_VERSION, REMOTE_WRITE_VERSION_01, TimeSeries,
    WriteRequest,
};
use reqwest::{
    StatusCode, Url,
    blocking::Client,
    header::{self, HeaderValue},
};
use tracing::{debug, info, warn};

use crate::USER_AGENT;

/// First delay before a failed block is sent again. The delay doubles on each
/// consecutive failure, up to `RETRY_MAX_INTERVAL` (same defaults as vmagent).
const RETRY_MIN_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_MAX_INTERVAL: Duration = Duration::from_secs(60);
/// Upper limit for a `Retry-After` value from the remote storage.
const RETRY_AFTER_MAX: Duration = Duration::from_secs(3600);
/// Maximum number of response body characters to put in a log message.
const BODY_LOG_LIMIT: usize = 512;

/// Result of one attempt to send a block.
#[derive(Debug)]
enum SendOutcome {
    /// The remote storage accepted the block.
    Accepted,
    /// The remote storage refused the block and a retry cannot succeed.
    Rejected(String),
    /// The attempt failed, but a later attempt can succeed.
    Retry {
        reason: String,
        retry_after: Option<Duration>,
    },
}

/// FIFO queue of encoded blocks that the remote storage has not accepted yet.
///
/// The queue is in memory only and has a byte limit. When it is over the limit,
/// the oldest blocks are dropped. The newest block is always kept.
struct PushQueue {
    blocks: VecDeque<Vec<u8>>,
    bytes: usize,
    max_bytes: usize,
    retry_delay: Duration,
    next_attempt: Option<Instant>,
}

impl PushQueue {
    fn new(max_bytes: usize) -> Self {
        Self {
            blocks: VecDeque::new(),
            bytes: 0,
            max_bytes,
            retry_delay: RETRY_MIN_INTERVAL,
            next_attempt: None,
        }
    }

    fn enqueue(&mut self, block: Vec<u8>) {
        self.bytes += block.len();
        self.blocks.push_back(block);

        let mut dropped = 0usize;
        while self.bytes > self.max_bytes && self.blocks.len() > 1 {
            if let Some(oldest) = self.blocks.pop_front() {
                self.bytes -= oldest.len();
                dropped += 1;
            }
        }
        if dropped > 0 {
            warn!(
                "push queue is full ({} bytes max); dropped the {dropped} oldest block(s)",
                self.max_bytes
            );
        }
    }

    fn pop_front(&mut self) {
        if let Some(block) = self.blocks.pop_front() {
            self.bytes -= block.len();
        }
    }

    /// Sends the queued blocks, oldest first, until the queue is empty or an
    /// attempt fails. A failed block stays in the queue for the next call.
    fn flush(&mut self, now: Instant, mut send: impl FnMut(&[u8]) -> SendOutcome) -> Result<()> {
        if let Some(next_attempt) = self.next_attempt
            && now < next_attempt
        {
            debug!(
                "retry backoff is active; {} block(s) stay in the queue",
                self.blocks.len()
            );
            return Ok(());
        }

        let backlog = self.blocks.len().saturating_sub(1);

        while let Some(block) = self.blocks.front() {
            match send(block) {
                SendOutcome::Accepted => self.pop_front(),
                SendOutcome::Rejected(reason) => {
                    warn!("remote storage rejected a block; dropped it: {reason}");
                    self.pop_front();
                }
                SendOutcome::Retry {
                    reason,
                    retry_after,
                } => {
                    let delay = retry_after
                        .map(|d| d.min(RETRY_AFTER_MAX))
                        .unwrap_or(self.retry_delay);
                    self.next_attempt = Some(now + delay);
                    self.retry_delay = (self.retry_delay * 2).min(RETRY_MAX_INTERVAL);
                    return Err(miette!(
                        "push failed: {reason}; {} block(s) queued ({} bytes), next attempt in {}s or later",
                        self.blocks.len(),
                        self.bytes,
                        delay.as_secs()
                    ));
                }
            }
            // The remote storage gave a final answer, thus it is reachable again.
            self.retry_delay = RETRY_MIN_INTERVAL;
            self.next_attempt = None;
        }

        if backlog > 0 {
            info!("push queue is empty again; processed {backlog} queued block(s)");
        }
        Ok(())
    }
}

pub struct PushClient {
    client: Client,
    url: Url,
    auth_header: Option<HeaderValue>,
    queue: PushQueue,
}

impl PushClient {
    pub fn new(
        url: &str,
        username: Option<&str>,
        password: Option<&str>,
        max_queue_bytes: usize,
    ) -> Result<Self> {
        let url = url.parse::<Url>().into_diagnostic()?;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .into_diagnostic()?;

        let auth_header = match (username, password) {
            (Some(username), Some(password)) => {
                let credentials = STANDARD.encode(format!("{username}:{password}"));
                let mut value: HeaderValue =
                    format!("Basic {credentials}").parse().into_diagnostic()?;
                value.set_sensitive(true);
                Some(value)
            }
            _ => None,
        };

        Ok(Self {
            client,
            url,
            auth_header,
            queue: PushQueue::new(max_queue_bytes),
        })
    }

    /// Adds the timeseries to the queue as one block, then sends the queue.
    ///
    /// Blocks that the remote storage could not receive stay in the queue and
    /// are sent again, oldest first, on a subsequent call.
    pub fn push(&mut self, timeseries: Vec<TimeSeries>) -> Result<()> {
        let block = WriteRequest { timeseries }
            .encode_compressed()
            .map_err(|err| miette!("failed to encode write request: {}", err))?;
        self.queue.enqueue(block);

        let Self {
            client,
            url,
            auth_header,
            queue,
        } = self;
        queue.flush(Instant::now(), |block| {
            send_block(client, url, auth_header.as_ref(), block)
        })
    }
}

fn send_block(
    client: &Client,
    url: &Url,
    auth_header: Option<&HeaderValue>,
    block: &[u8],
) -> SendOutcome {
    let mut req = client
        .post(url.clone())
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .header(header::CONTENT_ENCODING, "snappy")
        .header(HEADER_NAME_REMOTE_WRITE_VERSION, REMOTE_WRITE_VERSION_01)
        .header(header::USER_AGENT, USER_AGENT)
        .body(block.to_vec());

    if let Some(auth) = auth_header {
        req = req.header(header::AUTHORIZATION, auth.clone());
    }

    let response = match req.send() {
        Ok(response) => response,
        Err(err) => {
            return SendOutcome::Retry {
                reason: err.to_string(),
                retry_after: None,
            };
        }
    };

    let status = response.status();
    debug!("push response status: {status}");
    if status.is_success() {
        return SendOutcome::Accepted;
    }

    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_retry_after);
    let body: String = response
        .text()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(BODY_LOG_LIMIT)
        .collect();
    let reason = format!("status {status}; response body: {body:?}");

    if is_rejection(status) {
        SendOutcome::Rejected(reason)
    } else {
        SendOutcome::Retry {
            reason,
            retry_after,
        }
    }
}

/// Status codes for which the block is dropped, because the remote storage will
/// not accept the same block on a retry (same set as vmagent). All other
/// non-2xx codes are retried.
fn is_rejection(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT | StatusCode::UNSUPPORTED_MEDIA_TYPE
    )
}

/// Parses a `Retry-After` value in seconds. The HTTP-date form is ignored.
fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://localhost:9090/api/v1/write";

    fn retry() -> SendOutcome {
        SendOutcome::Retry {
            reason: "down".to_string(),
            retry_after: None,
        }
    }

    #[test]
    fn test_new_rejects_invalid_url() {
        assert!(PushClient::new("not a url", None, None, 0).is_err());
    }

    #[test]
    fn test_new_builds_basic_auth_header() {
        let client = PushClient::new(URL, Some("user"), Some("pass"), 0).unwrap();
        let auth = client.auth_header.expect("auth header should be set");
        assert_eq!(auth.to_str().unwrap(), "Basic dXNlcjpwYXNz");
        assert!(auth.is_sensitive());
    }

    #[test]
    fn test_new_skips_auth_without_password() {
        let client = PushClient::new(URL, Some("user"), None, 0).unwrap();
        assert!(client.auth_header.is_none());
    }

    #[test]
    fn test_flush_sends_oldest_first_and_empties_queue() {
        let mut queue = PushQueue::new(1024);
        queue.enqueue(vec![1]);
        queue.enqueue(vec![2]);
        queue.enqueue(vec![3]);

        let mut sent = Vec::new();
        let result = queue.flush(Instant::now(), |block| {
            sent.push(block.to_vec());
            SendOutcome::Accepted
        });

        assert!(result.is_ok());
        assert_eq!(sent, [vec![1], vec![2], vec![3]]);
        assert!(queue.blocks.is_empty());
        assert_eq!(queue.bytes, 0);
    }

    #[test]
    fn test_failed_block_stays_queued_and_stops_the_flush() {
        let mut queue = PushQueue::new(1024);
        queue.enqueue(vec![1]);
        queue.enqueue(vec![2]);

        let mut attempts = 0;
        let result = queue.flush(Instant::now(), |_| {
            attempts += 1;
            retry()
        });

        assert!(result.is_err());
        assert_eq!(attempts, 1);
        assert_eq!(queue.blocks, [vec![1], vec![2]]);
    }

    #[test]
    fn test_rejected_block_is_dropped_and_flush_continues() {
        let mut queue = PushQueue::new(1024);
        queue.enqueue(vec![1]);
        queue.enqueue(vec![2]);

        let mut sent = Vec::new();
        let result = queue.flush(Instant::now(), |block| {
            sent.push(block.to_vec());
            if block == [1] {
                SendOutcome::Rejected("bad".to_string())
            } else {
                SendOutcome::Accepted
            }
        });

        assert!(result.is_ok());
        assert_eq!(sent, [vec![1], vec![2]]);
        assert!(queue.blocks.is_empty());
        assert!(queue.next_attempt.is_none());
    }

    #[test]
    fn test_backoff_doubles_and_skips_attempts_until_due() {
        let mut queue = PushQueue::new(1024);
        queue.enqueue(vec![1]);
        let start = Instant::now();

        assert!(queue.flush(start, |_| retry()).is_err());
        assert_eq!(queue.next_attempt, Some(start + RETRY_MIN_INTERVAL));
        assert_eq!(queue.retry_delay, RETRY_MIN_INTERVAL * 2);

        // Not due yet: no attempt.
        let mut attempts = 0;
        let result = queue.flush(start + Duration::from_millis(500), |_| {
            attempts += 1;
            SendOutcome::Accepted
        });
        assert!(result.is_ok());
        assert_eq!(attempts, 0);
        assert_eq!(queue.blocks.len(), 1);

        // Due: second failure uses the doubled delay.
        let second = start + Duration::from_secs(1);
        assert!(queue.flush(second, |_| retry()).is_err());
        assert_eq!(queue.next_attempt, Some(second + RETRY_MIN_INTERVAL * 2));

        // Success sets the backoff to its initial state.
        let third = second + Duration::from_secs(2);
        assert!(queue.flush(third, |_| SendOutcome::Accepted).is_ok());
        assert_eq!(queue.retry_delay, RETRY_MIN_INTERVAL);
        assert!(queue.next_attempt.is_none());
        assert!(queue.blocks.is_empty());
    }

    #[test]
    fn test_backoff_does_not_exceed_max_interval() {
        let mut queue = PushQueue::new(1024);
        queue.enqueue(vec![1]);
        let mut now = Instant::now();
        for _ in 0..20 {
            assert!(queue.flush(now, |_| retry()).is_err());
            now = queue.next_attempt.unwrap();
        }
        assert_eq!(queue.retry_delay, RETRY_MAX_INTERVAL);
    }

    #[test]
    fn test_retry_after_overrides_backoff_delay_with_a_limit() {
        let mut queue = PushQueue::new(1024);
        queue.enqueue(vec![1]);
        let now = Instant::now();

        let outcome = |secs| SendOutcome::Retry {
            reason: "busy".to_string(),
            retry_after: Some(Duration::from_secs(secs)),
        };

        assert!(queue.flush(now, |_| outcome(120)).is_err());
        assert_eq!(queue.next_attempt, Some(now + Duration::from_secs(120)));

        let later = now + Duration::from_secs(120);
        assert!(queue.flush(later, |_| outcome(u64::MAX)).is_err());
        assert_eq!(queue.next_attempt, Some(later + RETRY_AFTER_MAX));
    }

    #[test]
    fn test_enqueue_drops_oldest_blocks_over_the_byte_limit() {
        let mut queue = PushQueue::new(5);
        queue.enqueue(vec![1, 1]);
        queue.enqueue(vec![2, 2]);
        queue.enqueue(vec![3, 3]);

        assert_eq!(queue.blocks, [vec![2, 2], vec![3, 3]]);
        assert_eq!(queue.bytes, 4);
    }

    #[test]
    fn test_enqueue_always_keeps_the_newest_block() {
        let mut queue = PushQueue::new(0);
        queue.enqueue(vec![1, 1]);
        queue.enqueue(vec![2, 2, 2]);

        assert_eq!(queue.blocks, [vec![2, 2, 2]]);
        assert_eq!(queue.bytes, 3);
    }

    #[test]
    fn test_only_400_409_415_are_rejections() {
        for code in [400, 409, 415] {
            assert!(is_rejection(StatusCode::from_u16(code).unwrap()), "{code}");
        }
        for code in [401, 403, 404, 408, 413, 429, 500, 502, 503] {
            assert!(!is_rejection(StatusCode::from_u16(code).unwrap()), "{code}");
        }
    }

    #[test]
    fn test_parse_retry_after_accepts_seconds_only() {
        assert_eq!(parse_retry_after(" 30 "), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after("-1"), None);
    }
}
