use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Error;

pub(crate) enum SseAction<T> {
    Yield(Result<T, Error>),
    Done,
    Skip,
}

/// Build a `Stream` of `T` from an HTTP response containing Server-Sent Events.
///
/// `parse_data` receives the `data:` payload of each SSE event and decides
/// what to do with it (yield an item, skip, or signal completion).
pub(crate) fn sse_stream<T, F>(
    response: reqwest::Response,
    parse_data: F,
) -> Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>
where
    T: Send + 'static,
    F: Fn(&str) -> SseAction<T> + Send + Sync + 'static,
{
    let byte_stream = response.bytes_stream();
    let parse_data = Arc::new(parse_data);

    Box::pin(futures::stream::unfold(
        (
            Box::pin(byte_stream)
                as Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
            String::new(),
            false,
        ),
        move |(mut stream, mut buffer, done)| {
            let parse_data = Arc::clone(&parse_data);
            async move {
                if done {
                    return None;
                }
                loop {
                    // Try to consume complete SSE event blocks from the buffer.
                    while let Some(pos) = buffer.find("\n\n") {
                        let block = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();

                        if let Some(data) = extract_sse_data(&block) {
                            match parse_data(&data) {
                                SseAction::Yield(item) => {
                                    return Some((item, (stream, buffer, false)));
                                }
                                SseAction::Done => return None,
                                SseAction::Skip => {}
                            }
                        }
                    }

                    // Need more bytes from the network.
                    match StreamExt::next(&mut stream).await {
                        Some(Ok(bytes)) => {
                            let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                            buffer.push_str(&text);
                        }
                        Some(Err(e)) => {
                            return Some((Err(Error::Http(e)), (stream, buffer, true)));
                        }
                        None => return None,
                    }
                }
            }
        },
    ))
}

/// Extract the concatenated `data:` field(s) from a single SSE event block.
fn extract_sse_data(block: &str) -> Option<String> {
    let mut parts = Vec::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            parts.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}
