# self-llm

## What This Project Is For

`self-llm` is a Rust library that provides a unified chat API across multiple LLM providers.

It is designed for applications that need to:

- talk to different providers through one request/response model
- support both regular and streaming chat flows
- handle tool calls, tool results, and multimodal content in a consistent way
- keep provider-specific differences inside adapter modules instead of application code

Current built-in adapters:

- OpenAI / OpenAI-compatible APIs
- Anthropic / Anthropic-compatible APIs

## Features

- One `Client` entry point
- Unified `ChatRequest` and `ChatResponse` types
- Streaming support through `StreamEvent`
- Text, reasoning, image, tool-call, and tool-result content support
- Builder-style provider and model configuration

## Installation

If the crate is published on crates.io:

```toml
[dependencies]
self-llm = "0.1"
```

For local development:

```toml
[dependencies]
self-llm = { path = "." }
```

## Quick Start

### 1. Create an OpenAI client directly

```rust
use self_llm::{ChatRequest, Client, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::openai(std::env::var("OPENAI_API_KEY")?);

    let request = ChatRequest::new(
        "gpt-4.1-mini",
        vec![
            Message::system("You are a helpful assistant."),
            Message::user("Explain Rust ownership in one paragraph."),
        ],
    )
    .max_tokens(512)
    .temperature(0.7);

    let response = client.chat(request).await?;

    println!("text: {}", response.text().unwrap_or(""));
    println!("reasoning: {:?}", response.reasoning());
    println!("stop_reason: {:?}", response.stop_reason);

    Ok(())
}
```

### 2. Build a client from provider config

```rust
use self_llm::{ChatRequest, LlmProviderConfig, Message, ProviderType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = LlmProviderConfig::new(
        "my-openai",
        "https://api.openai.com/v1",
        ProviderType::OpenAi,
        std::env::var("OPENAI_API_KEY")?,
    );

    let client = provider.build_client();

    let request = ChatRequest::new(
        "gpt-4.1-mini",
        vec![Message::user("Hello from self-llm")],
    );

    let response = client.chat(request).await?;
    println!("{}", response.text().unwrap_or(""));

    Ok(())
}
```

### 3. Streaming

```rust
use futures::StreamExt;
use self_llm::{ChatRequest, Client, Message, StreamEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::openai(std::env::var("OPENAI_API_KEY")?);

    let request = ChatRequest::new(
        "gpt-4.1-mini",
        vec![Message::user("Write a short haiku about Rust.")],
    );

    let mut stream = client.chat_stream(request).await?;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::ContentDelta(text) => print!("{}", text),
            StreamEvent::ReasoningDelta(reasoning) => {
                eprintln!("reasoning: {}", reasoning);
            }
            StreamEvent::Done(reason) => {
                eprintln!("\nstop_reason: {:?}", reason);
            }
            _ => {}
        }
    }

    Ok(())
}
```

### 4. Tool calling

```rust
use self_llm::{ChatRequest, Client, Message, Tool, ToolResult};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::openai(std::env::var("OPENAI_API_KEY")?);

    let request = ChatRequest::new(
        "gpt-4.1-mini",
        vec![
            Message::system("Use tools when solving arithmetic problems."),
            Message::user("Please calculate 7 + 5."),
        ],
    )
    .tools(vec![Tool {
        name: "calculate".to_string(),
        description: "Perform basic arithmetic".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "operation": { "type": "string" },
                "a": { "type": "number" },
                "b": { "type": "number" }
            },
            "required": ["operation", "a", "b"]
        }),
    }]);

    let first = client.chat(request).await?;

    for tool_use in first.tool_uses() {
        println!("tool call: {} -> {}", tool_use.id, tool_use.name);
    }

    let tool_results = vec![ToolResult {
        tool_use_id: first.tool_uses()[0].id.clone(),
        content: json!({ "result": 12 }).to_string(),
        is_error: false,
    }];

    let followup = ChatRequest::new(
        "gpt-4.1-mini",
        vec![
            Message::user("Please calculate 7 + 5."),
            self_llm::Message {
                role: self_llm::Role::Assistant,
                content: first.content.clone(),
            },
            Message::tool_results(tool_results),
        ],
    );

    let final_response = client.chat(followup).await?;
    println!("final: {}", final_response.text().unwrap_or(""));

    Ok(())
}
```

For a fuller roundtrip example, see `tests/integration_test.rs`.

## Configuration

`LlmConfig` describes model capabilities and defaults such as:

- `thinking`
- `image_understanding`
- `struct_output`
- `tool_use`
- `temperature`
- `top_p`

`LlmProviderConfig` describes provider connection details such as:

- `provider_name`
- `base_url`
- `provider_type`
- `api_key`
- `custom_header`

## Development

Default validation commands:

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Notes:

- `tests/integration_test.rs` loads secrets from `.env` using `dotenvy`
- some tests call live provider APIs
- if you are only changing conversion logic or internal behavior, prefer linting and local compile checks first

## Repository Layout

```text
src/lib.rs             Public API exports
src/client.rs          Unified client and provider dispatch
src/types.rs           Provider-agnostic request/response types
src/config.rs          Builder-style provider/model configuration
src/openai/            OpenAI adapter
src/anthropic/         Anthropic adapter
src/sse.rs             Shared SSE stream parsing
tests/integration_test.rs   Realistic usage and integration coverage
```