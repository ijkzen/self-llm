mod types;

use std::collections::HashMap;
use std::pin::Pin;

use futures::Stream;

use crate::{
    error::Error,
    sse::{self, SseAction},
    types as unified,
};

const DEFAULT_MAX_TOKENS: u32 = 4096;

pub(crate) async fn chat(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    api_version: &str,
    custom_headers: &HashMap<String, String>,
    request: unified::ChatRequest,
) -> Result<unified::ChatResponse, Error> {
    let body = convert_request(request, false);
    let url = format!("{}/v1/messages", base_url);

    let mut req = http
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", api_version)
        .json(&body);
    for (k, v) in custom_headers {
        req = req.header(k, v);
    }
    let resp = req.send().await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }

    let api_resp: types::Response = resp.json().await?;
    Ok(convert_response(api_resp))
}

pub(crate) async fn chat_stream(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    api_version: &str,
    custom_headers: &HashMap<String, String>,
    request: unified::ChatRequest,
) -> Result<Pin<Box<dyn Stream<Item = Result<unified::StreamEvent, Error>> + Send>>, Error> {
    let body = convert_request(request, true);
    let url = format!("{}/v1/messages", base_url);

    let mut req = http
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", api_version)
        .json(&body);
    for (k, v) in custom_headers {
        req = req.header(k, v);
    }
    let resp = req.send().await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(Error::Api { status, message });
    }

    Ok(sse::sse_stream(resp, parse_stream_data))
}

// ---------------------------------------------------------------------------
// Streaming parser
// ---------------------------------------------------------------------------

fn parse_stream_data(data: &str) -> SseAction<unified::StreamEvent> {
    let event: types::StreamEvent = match serde_json::from_str(data) {
        Ok(e) => e,
        Err(e) => return SseAction::Yield(Err(Error::Json(e))),
    };

    match event {
        types::StreamEvent::ContentBlockDelta { index, delta } => match delta {
            types::Delta::TextDelta { text } => {
                SseAction::Yield(Ok(unified::StreamEvent::ContentDelta(text)))
            }
            types::Delta::InputJsonDelta { partial_json } => {
                SseAction::Yield(Ok(unified::StreamEvent::ToolCallDelta {
                    index,
                    arguments_delta: partial_json,
                }))
            }
        },
        types::StreamEvent::ContentBlockStart {
            index,
            content_block,
        } => {
            if let types::ContentBlock::ToolUse { id, name, .. } = content_block {
                SseAction::Yield(Ok(unified::StreamEvent::ToolCallStart { index, id, name }))
            } else {
                SseAction::Skip
            }
        }
        types::StreamEvent::MessageStart { message } => {
            SseAction::Yield(Ok(unified::StreamEvent::Usage(unified::Usage {
                input_tokens: message.usage.input_tokens,
                output_tokens: message.usage.output_tokens,
            })))
        }
        types::StreamEvent::MessageDelta { delta, .. } => {
            let stop = match delta.stop_reason.as_deref() {
                Some("end_turn") => unified::StopReason::EndTurn,
                Some("max_tokens") => unified::StopReason::MaxTokens,
                Some("tool_use") => unified::StopReason::ToolUse,
                Some(other) => unified::StopReason::Unknown(other.to_string()),
                None => unified::StopReason::Unknown("none".to_string()),
            };
            SseAction::Yield(Ok(unified::StreamEvent::Done(stop)))
        }
        types::StreamEvent::Error { error } => {
            SseAction::Yield(Err(Error::Stream(error.message)))
        }
        types::StreamEvent::MessageStop
        | types::StreamEvent::ContentBlockStop { .. }
        | types::StreamEvent::Ping => SseAction::Skip,
    }
}

// ---------------------------------------------------------------------------
// Request / response conversion
// ---------------------------------------------------------------------------

fn convert_request(request: unified::ChatRequest, stream: bool) -> types::Request {
    let mut system = None;
    let mut messages = Vec::new();

    for msg in &request.messages {
        if msg.role == unified::Role::System {
            // Anthropic: system prompt is a top-level parameter, not a message.
            let text: String = msg
                .content
                .iter()
                .filter_map(|p| match p {
                    unified::ContentPart::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            system = Some(text);
            continue;
        }

        let role = match msg.role {
            unified::Role::User => "user",
            unified::Role::Assistant => "assistant",
            unified::Role::System => unreachable!(),
        };

        messages.push(types::Message {
            role: role.to_string(),
            content: convert_content(&msg.content),
        });
    }

    let tools = request.tools.map(|tools| {
        tools
            .into_iter()
            .map(|t| types::Tool {
                name: t.name,
                description: t.description,
                input_schema: t.parameters,
            })
            .collect()
    });

    types::Request {
        model: request.model,
        messages,
        max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        system,
        temperature: request.temperature,
        top_p: request.top_p,
        tools,
        stream,
    }
}

fn convert_content(parts: &[unified::ContentPart]) -> Vec<types::ContentBlock> {
    parts
        .iter()
        .map(|p| match p {
            unified::ContentPart::Text(t) => types::ContentBlock::Text { text: t.clone() },
            unified::ContentPart::Reasoning(_) => {
                types::ContentBlock::Text { text: String::new() }
            }
            unified::ContentPart::Image(src) => {
                let source = match src {
                    unified::ImageSource::Url(url) => {
                        types::ImageSource::Url { url: url.clone() }
                    }
                    unified::ImageSource::Base64 { media_type, data } => {
                        types::ImageSource::Base64 {
                            media_type: media_type.clone(),
                            data: data.clone(),
                        }
                    }
                };
                types::ContentBlock::Image { source }
            }
            unified::ContentPart::ToolUse(tu) => types::ContentBlock::ToolUse {
                id: tu.id.clone(),
                name: tu.name.clone(),
                input: tu.input.clone(),
            },
            unified::ContentPart::ToolResult(tr) => types::ContentBlock::ToolResult {
                tool_use_id: tr.tool_use_id.clone(),
                content: tr.content.clone(),
                is_error: tr.is_error,
            },
        })
        .filter(|block| match block {
            types::ContentBlock::Text { text } => !text.is_empty(),
            _ => true,
        })
        .collect()
}

fn convert_response(resp: types::Response) -> unified::ChatResponse {
    let content: Vec<unified::ContentPart> = resp
        .content
        .into_iter()
        .filter_map(|block| match block {
            types::ContentBlock::Text { text } => Some(unified::ContentPart::Text(text)),
            types::ContentBlock::ToolUse { id, name, input } => {
                Some(unified::ContentPart::ToolUse(unified::ToolUse {
                    id,
                    name,
                    input,
                }))
            }
            // Image and ToolResult don't appear in assistant responses.
            _ => None,
        })
        .collect();

    let stop_reason = match resp.stop_reason.as_deref() {
        Some("end_turn") => unified::StopReason::EndTurn,
        Some("max_tokens") => unified::StopReason::MaxTokens,
        Some("tool_use") => unified::StopReason::ToolUse,
        Some(other) => unified::StopReason::Unknown(other.to_string()),
        None => unified::StopReason::Unknown("none".to_string()),
    };

    unified::ChatResponse {
        id: resp.id,
        model: resp.model,
        content,
        usage: Some(unified::Usage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        }),
        stop_reason,
    }
}
