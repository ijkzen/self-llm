# self-llm

## 项目用途

`self-llm` 是一个 Rust 库，用统一的 API 封装多个大语言模型提供商。

当前仓库主要面向这类场景：

- 希望用同一套请求/响应结构接入不同供应商
- 需要同时支持普通对话和流式输出
- 需要在统一模型下处理工具调用、工具结果回传和多模态消息
- 希望把供应商差异收敛在适配层，而不是散落到业务代码里

当前已内置的适配器：

- OpenAI / OpenAI-compatible API
- Anthropic / Anthropic-compatible API

## 主要能力

- 统一的 `Client` 入口
- 统一的 `ChatRequest` / `ChatResponse` 数据结构
- 支持流式事件 `StreamEvent`
- 支持文本、推理内容、图片、工具调用、工具结果
- 支持通过 `LlmProviderConfig` 和 `LlmConfig` 组织模型与提供商配置

## 安装

如果已经发布到 crates.io：

```toml
[dependencies]
self-llm = "0.1"
```

如果你在本地仓库中使用：

```toml
[dependencies]
self-llm = { path = "." }
```

## 快速开始

### 1. 直接创建 OpenAI 客户端

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

### 2. 使用提供商配置构建客户端

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

### 3. 流式输出

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

### 4. 工具调用

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
            Message::user("Please calculate 7 + 5.")
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

完整的工具调用往返示例可以参考 `tests/integration_test.rs`。

## 配置说明

`LlmConfig` 用于描述模型能力和默认采样参数，例如：

- `thinking`
- `image_understanding`
- `struct_output`
- `tool_use`
- `temperature`
- `top_p`

`LlmProviderConfig` 用于描述供应商接入信息，例如：

- `provider_name`
- `base_url`
- `provider_type`
- `api_key`
- `custom_header`

## 开发与验证

默认验证命令：

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

说明：

- `tests/integration_test.rs` 会通过 `dotenvy` 读取 `.env`
- 部分测试会调用真实模型 API
- 如果你只是修改类型转换或静态逻辑，优先跑 `clippy` 和本地编译检查

## 仓库结构

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