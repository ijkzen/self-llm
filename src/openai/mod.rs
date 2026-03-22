mod types;

use std::collections::HashMap;
use std::pin::Pin;

use futures::Stream;

use crate::{
    error::Error,
    sse::{self, SseAction},
    types as unified,
};

pub(crate) async fn chat(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    custom_headers: &HashMap<String, String>,
    request: unified::ChatRequest,
) -> Result<unified::ChatResponse, Error> {
    let body = convert_request(request, false);
    let url = format!("{}/chat/completions", base_url);

    let mut req = http.post(&url).bearer_auth(api_key).json(&body);
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
    custom_headers: &HashMap<String, String>,
    request: unified::ChatRequest,
) -> Result<Pin<Box<dyn Stream<Item = Result<unified::StreamEvent, Error>> + Send>>, Error> {
    let body = convert_request(request, true);
    let url = format!("{}/chat/completions", base_url);

    let mut req = http.post(&url).bearer_auth(api_key).json(&body);
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
    if data.trim() == "[DONE]" {
        return SseAction::Done;
    }

    let chunk: types::StreamChunk = match serde_json::from_str(data) {
        Ok(c) => c,
        Err(e) => return SseAction::Yield(Err(Error::Json(e))),
    };

    // Usage-only chunk (sent at the end when include_usage is set).
    if let Some(usage) = &chunk.usage {
        return SseAction::Yield(Ok(unified::StreamEvent::Usage(unified::Usage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        })));
    }

    let Some(choice) = chunk.choices.first() else {
        return SseAction::Skip;
    };

    // Finish reason.
    if let Some(reason) = &choice.finish_reason {
        let stop = match reason.as_str() {
            "stop" => unified::StopReason::EndTurn,
            "length" => unified::StopReason::MaxTokens,
            "tool_calls" => unified::StopReason::ToolUse,
            other => unified::StopReason::Unknown(other.to_string()),
        };
        return SseAction::Yield(Ok(unified::StreamEvent::Done(stop)));
    }

    // Text content delta.
    if let Some(text) = &choice.delta.content
        && !text.is_empty()
    {
        return SseAction::Yield(Ok(unified::StreamEvent::ContentDelta(text.clone())));
    }

    // Reasoning content delta.
    if let Some(reasoning) = &choice.delta.reasoning_content
        && !reasoning.is_empty()
    {
        return SseAction::Yield(Ok(unified::StreamEvent::ReasoningDelta(reasoning.clone())));
    }

    // Tool-call deltas.
    if let Some(tool_calls) = &choice.delta.tool_calls {
        for tc in tool_calls {
            if let Some(func) = &tc.function {
                if let (Some(id), Some(name)) = (&tc.id, &func.name) {
                    return SseAction::Yield(Ok(unified::StreamEvent::ToolCallStart {
                        index: tc.index,
                        id: id.clone(),
                        name: name.clone(),
                    }));
                }
                if let Some(args) = &func.arguments
                    && !args.is_empty()
                {
                    return SseAction::Yield(Ok(unified::StreamEvent::ToolCallDelta {
                        index: tc.index,
                        arguments_delta: args.clone(),
                    }));
                }
            }
        }
    }

    SseAction::Skip
}

// ---------------------------------------------------------------------------
// Request / response conversion
// ---------------------------------------------------------------------------

fn convert_request(request: unified::ChatRequest, stream: bool) -> types::Request {
    let mut messages = Vec::new();

    for msg in &request.messages {
        let has_tool_results = msg
            .content
            .iter()
            .any(|p| matches!(p, unified::ContentPart::ToolResult(_)));

        if has_tool_results {
            // Each ToolResult becomes a separate "tool" role message.
            for part in &msg.content {
                if let unified::ContentPart::ToolResult(tr) = part {
                    messages.push(types::Message {
                        role: "tool".to_string(),
                        content: Some(types::Content::Text(tr.content.clone())),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(tr.tool_use_id.clone()),
                    });
                }
            }
            continue;
        }

        let has_tool_use = msg
            .content
            .iter()
            .any(|p| matches!(p, unified::ContentPart::ToolUse(_)));

        if has_tool_use {
            // Assistant message with tool calls.
            let text = msg.content.iter().find_map(|p| match p {
                unified::ContentPart::Text(t) => Some(t.clone()),
                _ => None,
            });
            let reasoning = msg.content.iter().find_map(|p| match p {
                unified::ContentPart::Reasoning(t) => Some(t.clone()),
                _ => None,
            });
            let tool_calls: Vec<types::ToolCall> = msg
                .content
                .iter()
                .filter_map(|p| match p {
                    unified::ContentPart::ToolUse(tu) => Some(types::ToolCall {
                        id: tu.id.clone(),
                        call_type: "function".to_string(),
                        function: types::FunctionCall {
                            name: tu.name.clone(),
                            arguments: tu.input.to_string(),
                        },
                    }),
                    _ => None,
                })
                .collect();

            messages.push(types::Message {
                role: role_str(&msg.role),
                content: text.map(types::Content::Text),
                reasoning_content: reasoning,
                tool_calls: Some(tool_calls),
                tool_call_id: None,
            });
            continue;
        }

        // Regular message.
        messages.push(types::Message {
            role: role_str(&msg.role),
            content: Some(convert_content(&msg.content)),
            reasoning_content: msg.content.iter().find_map(|p| match p {
                unified::ContentPart::Reasoning(t) => Some(t.clone()),
                _ => None,
            }),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    let tools = request.tools.map(|tools| {
        tools
            .into_iter()
            .map(|t| types::Tool {
                tool_type: "function".to_string(),
                function: types::FunctionDef {
                    name: t.name,
                    description: t.description,
                    parameters: t.parameters,
                },
            })
            .collect()
    });

    types::Request {
        model: request.model,
        messages,
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        top_p: request.top_p,
        tools,
        stream,
        stream_options: if stream {
            Some(types::StreamOptions {
                include_usage: true,
            })
        } else {
            None
        },
    }
}

fn role_str(role: &unified::Role) -> String {
    match role {
        unified::Role::System => "system".to_string(),
        unified::Role::User => "user".to_string(),
        unified::Role::Assistant => "assistant".to_string(),
    }
}

fn convert_content(parts: &[unified::ContentPart]) -> types::Content {
    // Single text → simple string (more compact).
    if parts.len() == 1
        && let unified::ContentPart::Text(t) = &parts[0]
    {
        return types::Content::Text(t.clone());
    }

    let openai_parts: Vec<types::ContentPart> = parts
        .iter()
        .filter_map(|p| match p {
            unified::ContentPart::Text(t) => Some(types::ContentPart::Text { text: t.clone() }),
            unified::ContentPart::Image(src) => {
                let url = match src {
                    unified::ImageSource::Url(u) => u.clone(),
                    unified::ImageSource::Base64 { media_type, data } => {
                        format!("data:{media_type};base64,{data}")
                    }
                };
                Some(types::ContentPart::ImageUrl {
                    image_url: types::ImageUrl { url },
                })
            }
            unified::ContentPart::Reasoning(_) => None,
            _ => None, // ToolUse/ToolResult handled above
        })
        .collect();

    types::Content::Parts(openai_parts)
}

fn convert_response(resp: types::Response) -> unified::ChatResponse {
    let choice = resp.choices.into_iter().next();
    let (content, stop_reason) = match choice {
        Some(c) => {
            let mut parts = Vec::new();
            if let Some(reasoning) = c.message.reasoning_content
                && !reasoning.is_empty()
            {
                parts.push(unified::ContentPart::Reasoning(reasoning));
            }
            if let Some(text) = c.message.content
                && !text.is_empty()
            {
                parts.push(unified::ContentPart::Text(text));
            }
            if let Some(tool_calls) = c.message.tool_calls {
                for tc in tool_calls {
                    let input = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::String(tc.function.arguments.clone()));
                    parts.push(unified::ContentPart::ToolUse(unified::ToolUse {
                        id: tc.id,
                        name: tc.function.name,
                        input,
                    }));
                }
            }
            let reason = match c.finish_reason.as_deref() {
                Some("stop") => unified::StopReason::EndTurn,
                Some("length") => unified::StopReason::MaxTokens,
                Some("tool_calls") => unified::StopReason::ToolUse,
                Some(other) => unified::StopReason::Unknown(other.to_string()),
                None => unified::StopReason::Unknown("none".to_string()),
            };
            (parts, reason)
        }
        None => (
            vec![],
            unified::StopReason::Unknown("no_choices".to_string()),
        ),
    };

    unified::ChatResponse {
        id: resp.id,
        model: resp.model,
        content,
        usage: resp.usage.map(|u| unified::Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        }),
        stop_reason,
    }
}
