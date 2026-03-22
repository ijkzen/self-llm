use futures::StreamExt;
use self_llm::{
    ChatRequest, LlmConfig, LlmProviderConfig, Message, ProviderType, Role, StopReason, Tool,
    ToolResult, ToolUse,
};
use serde_json::json;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

fn assert_response_is_valid(response: &self_llm::ChatResponse, model: &LlmConfig, label: &str) {
    assert!(
        !model.model_id.trim().is_empty(),
        "{label} model id should not be empty"
    );

    let text = response.text().unwrap_or("");
    let generated_tokens = response
        .usage
        .as_ref()
        .map(|usage| usage.output_tokens)
        .unwrap_or_default();

    assert!(
        !text.trim().is_empty() || !response.content.is_empty() || generated_tokens > 0,
        "{label} response should contain visible content or generation metadata: {response:?}"
    );
}

fn load_env() {
    dotenvy::dotenv().ok();
}

fn kimi_openai_provider() -> LlmProviderConfig {
    let api_key = std::env::var("KIMI_OPENAI_API_KEY").expect("KIMI_OPENAI_API_KEY not set");
    LlmProviderConfig::new(
        "kimi-openai",
        "https://api.moonshot.cn/v1",
        ProviderType::OpenAi,
        api_key,
    )
}

fn kimi_anthropic_provider() -> LlmProviderConfig {
    let api_key = std::env::var("KIMI_ANTHROPIC_API_KEY").expect("KIMI_ANTHROPIC_API_KEY not set");
    LlmProviderConfig::new(
        "kimi-anthropic",
        "https://api.kimi.com/coding",
        ProviderType::Anthropic,
        api_key,
    )
    .custom_header("User-Agent", "KimiCLI/1.3")
}

fn kimi_k2_5_model() -> LlmConfig {
    LlmConfig::new("kimi-k2.5", "kimi-k2.5", 98304, 258048, 262144)
        .thinking(true)
        .image_understanding(true)
        .struct_output(true)
        .tool_use(true)
        .temperature(1.0)
        .top_p(0.95)
}

fn build_request(model: &LlmConfig) -> ChatRequest {
    ChatRequest::new(
        &model.model_id,
        vec![
            Message::system("你是一个人工智能助手，协助用户完成各种任务。使用中文回答用户问题，并且在适当的时候使用工具。"),
            Message::user("你现在能工作吗？"),
        ],
    )
    .max_tokens(32768)
    .temperature(model.temperature)
    .top_p(model.top_p)
}

fn build_tool_request(model: &LlmConfig) -> ChatRequest {
    ChatRequest::new(
        &model.model_id,
        vec![
            Message::system(
                "你是一个严格遵循工具调用流程的助手。遇到算术题时必须调用工具，不要直接心算。",
            ),
            Message::user("请使用 calculate 工具分别计算 7 + 5 和 10 - 3，然后再用中文总结结果。"),
        ],
    )
    .max_tokens(32768)
    .temperature(model.temperature)
    .top_p(model.top_p)
    .tools(vec![calculator_tool()])
}

fn calculator_tool() -> Tool {
    Tool {
        name: "calculate".to_string(),
        description: "执行基础算术运算，目前支持 add 和 subtract".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["add", "subtract"],
                    "description": "算术运算类型"
                },
                "a": {
                    "type": "number",
                    "description": "第一个操作数"
                },
                "b": {
                    "type": "number",
                    "description": "第二个操作数"
                }
            },
            "required": ["operation", "a", "b"],
            "additionalProperties": false
        }),
    }
}

fn execute_calculator_tool(tool_use: &ToolUse) -> ToolResult {
    let operation = tool_use
        .input
        .get("operation")
        .and_then(|value| value.as_str())
        .expect("tool input missing operation");
    let a = tool_use
        .input
        .get("a")
        .and_then(|value| value.as_f64())
        .expect("tool input missing a");
    let b = tool_use
        .input
        .get("b")
        .and_then(|value| value.as_f64())
        .expect("tool input missing b");

    let result = match operation {
        "add" => a + b,
        "subtract" => a - b,
        other => panic!("unsupported operation: {other}"),
    };

    ToolResult {
        tool_use_id: tool_use.id.clone(),
        content: json!({
            "operation": operation,
            "a": a,
            "b": b,
            "result": result
        })
        .to_string(),
        is_error: false,
    }
}

async fn run_tool_call_roundtrip(client: &self_llm::Client, model: &LlmConfig, label: &str) {
    let initial_request = build_tool_request(model);
    let initial_messages = initial_request.messages.clone();
    let followup_tools = initial_request.tools.clone();

    let first_response = client
        .chat(initial_request)
        .await
        .expect("initial tool call request failed");

    let tool_uses: Vec<ToolUse> = first_response.tool_uses().into_iter().cloned().collect();
    assert!(
        !tool_uses.is_empty(),
        "{label} should return at least one tool call, response: {first_response:?}"
    );
    assert!(
        first_response.stop_reason == StopReason::ToolUse || !tool_uses.is_empty(),
        "{label} should stop for tool use or include tool calls"
    );

    let tool_results: Vec<ToolResult> = tool_uses.iter().map(execute_calculator_tool).collect();

    let mut followup_messages = initial_messages;
    followup_messages.push(Message {
        role: Role::Assistant,
        content: first_response.content.clone(),
    });
    followup_messages.push(Message::tool_results(tool_results));

    let followup_request = ChatRequest::new(&model.model_id, followup_messages)
        .max_tokens(1024)
        .temperature(model.temperature)
        .top_p(model.top_p)
        .tools(followup_tools.expect("tool definition missing in followup request"));

    let final_response = client
        .chat(followup_request)
        .await
        .expect("tool result followup request failed");

    assert_response_is_valid(&final_response, model, label);

    let final_text = final_response.text().unwrap_or("");
    assert!(
        final_text.contains("12") && final_text.contains("7"),
        "{label} final response should include calculation results 12 and 7, got: {final_response:?}"
    );
    println!("[{label}] final response: {final_text}");
}

// ============================================================================
// kimi-openai + kimi-k2.5
// ============================================================================

#[test]
fn test_kimi_openai_chat() {
    load_env();
    runtime().block_on(async {
        let provider = kimi_openai_provider();
        let model = kimi_k2_5_model();
        let client = provider.build_client();
        let request = build_request(&model);

        let response = client.chat(request).await.expect("chat request failed");
        assert_response_is_valid(&response, &model, "kimi-openai chat");
        println!("[kimi-openai chat] {:?}", response.text());
    });
}

#[test]
fn test_kimi_openai_chat_stream() {
    load_env();
    runtime().block_on(async {
        let provider = kimi_openai_provider();
        let model = kimi_k2_5_model();
        let client = provider.build_client();
        let request = build_request(&model);

        let mut stream = client
            .chat_stream(request)
            .await
            .expect("stream request failed");

        let mut full_text = String::new();
        let mut saw_usage = false;
        let mut saw_done = false;
        while let Some(event) = stream.next().await {
            match event.expect("stream event error") {
                self_llm::StreamEvent::ContentDelta(text) => {
                    full_text.push_str(&text);
                }
                self_llm::StreamEvent::Usage(_) => {
                    saw_usage = true;
                }
                self_llm::StreamEvent::Done(reason) => {
                    saw_done = true;
                    println!("[kimi-openai stream] stop_reason: {reason:?}");
                }
                _ => {}
            }
        }
        assert!(
            !full_text.trim().is_empty() || saw_usage || saw_done,
            "stream should yield visible text or valid completion events"
        );
        println!("[kimi-openai stream] {full_text}");
    });
}

#[test]
fn test_kimi_openai_tool_call() {
    load_env();
    runtime().block_on(async {
        let provider = kimi_openai_provider();
        let model = kimi_k2_5_model();
        let client = provider.build_client();

        run_tool_call_roundtrip(&client, &model, "kimi-openai tool call").await;
    });
}

// ============================================================================
// kimi-anthropic + kimi-k2.5
// ============================================================================

#[test]
fn test_kimi_anthropic_chat() {
    load_env();
    runtime().block_on(async {
        let provider = kimi_anthropic_provider();
        let model = kimi_k2_5_model();
        let client = provider.build_client();
        let request = build_request(&model);

        let response = client.chat(request).await.expect("chat request failed");
        assert_response_is_valid(&response, &model, "kimi-anthropic chat");
        println!("[kimi-anthropic chat] {:?}", response.text());
    });
}

#[test]
fn test_kimi_anthropic_chat_stream() {
    load_env();
    runtime().block_on(async {
        let provider = kimi_anthropic_provider();
        let model = kimi_k2_5_model();
        let client = provider.build_client();
        let request = build_request(&model);

        let mut stream = client
            .chat_stream(request)
            .await
            .expect("stream request failed");

        let mut full_text = String::new();
        let mut saw_usage = false;
        let mut saw_done = false;
        while let Some(event) = stream.next().await {
            match event.expect("stream event error") {
                self_llm::StreamEvent::ContentDelta(text) => {
                    full_text.push_str(&text);
                }
                self_llm::StreamEvent::Usage(_) => {
                    saw_usage = true;
                }
                self_llm::StreamEvent::Done(reason) => {
                    saw_done = true;
                    println!("[kimi-anthropic stream] stop_reason: {reason:?}");
                }
                _ => {}
            }
        }
        assert!(
            !full_text.trim().is_empty() || saw_usage || saw_done,
            "stream should yield visible text or valid completion events"
        );
        println!("[kimi-anthropic stream] {full_text}");
    });
}

#[test]
fn test_kimi_anthropic_tool_call() {
    load_env();
    runtime().block_on(async {
        let provider = kimi_anthropic_provider();
        let model = kimi_k2_5_model();
        let client = provider.build_client();

        run_tool_call_roundtrip(&client, &model, "kimi-anthropic tool call").await;
    });
}
