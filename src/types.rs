use serde::{Deserialize, Serialize};

/// Role of a message participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A part of message content.
#[derive(Debug, Clone)]
pub enum ContentPart {
    Text(String),
    Reasoning(String),
    Image(ImageSource),
    ToolUse(ToolUse),
    ToolResult(ToolResult),
}

/// Source data for an image.
#[derive(Debug, Clone)]
pub enum ImageSource {
    Url(String),
    Base64 { media_type: String, data: String },
}

/// A tool invocation returned by the model.
#[derive(Debug, Clone)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Result of a tool invocation, sent back to the model.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

/// A single message in the conversation.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![ContentPart::Text(text.into())],
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentPart::Text(text.into())],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentPart::Text(text.into())],
        }
    }

    pub fn assistant_with_reasoning(reasoning: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![
                ContentPart::Reasoning(reasoning.into()),
                ContentPart::Text(text.into()),
            ],
        }
    }

    pub fn user_with_image(text: impl Into<String>, image: ImageSource) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentPart::Text(text.into()), ContentPart::Image(image)],
        }
    }

    pub fn tool_results(results: Vec<ToolResult>) -> Self {
        Self {
            role: Role::User,
            content: results.into_iter().map(ContentPart::ToolResult).collect(),
        }
    }
}

/// Strategy for tool selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolChoice {
    /// Let the model decide whether to call tools.
    Auto,
    /// Never call tools.
    None,
    /// Force the model to call at least one tool (OpenAI: "required", Anthropic: "any").
    Required,
    /// Force the model to call a specific tool by name.
    Specific(String),
}

/// A tool definition that the model can call.
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool parameters.
    pub parameters: serde_json::Value,
}

/// A chat completion request.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub tools: Option<Vec<Tool>>,
    /// Strategy for tool selection.
    pub tool_choice: Option<ToolChoice>,
    /// Whether to allow parallel tool calls.
    /// OpenAI: `parallel_tool_calls`; Anthropic: `tool_choice.disable_parallel_tool_use`.
    pub parallel_tool_calls: Option<bool>,
    /// Custom stop sequences.
    /// OpenAI: `stop`; Anthropic: `stop_sequences`.
    pub stop_sequences: Option<Vec<String>>,
    /// Top-k sampling parameter (Anthropic only; ignored for OpenAI).
    pub top_k: Option<u32>,
    /// End-user identifier for abuse detection.
    /// OpenAI: `user`; Anthropic: `metadata.user_id`.
    pub user: Option<String>,
    /// Budget tokens for extended thinking (Anthropic).
    pub budget_tokens: Option<u32>,
    /// Enable prompt caching.
    /// OpenAI: sets `store: true`; Anthropic: sets top-level `cache_control`.
    pub prompt_cache: Option<bool>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            max_tokens: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            stop_sequences: None,
            top_k: None,
            user: None,
            budget_tokens: None,
            prompt_cache: None,
        }
    }

    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }

    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }

    pub fn top_p(mut self, p: f32) -> Self {
        self.top_p = Some(p);
        self
    }

    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = Some(choice);
        self
    }

    pub fn parallel_tool_calls(mut self, enabled: bool) -> Self {
        self.parallel_tool_calls = Some(enabled);
        self
    }

    pub fn stop_sequences(mut self, sequences: Vec<String>) -> Self {
        self.stop_sequences = Some(sequences);
        self
    }

    pub fn top_k(mut self, k: u32) -> Self {
        self.top_k = Some(k);
        self
    }

    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    pub fn budget_tokens(mut self, n: u32) -> Self {
        self.budget_tokens = Some(n);
        self
    }

    pub fn prompt_cache(mut self, enabled: bool) -> Self {
        self.prompt_cache = Some(enabled);
        self
    }
}

/// A chat completion response.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub content: Vec<ContentPart>,
    pub usage: Option<Usage>,
    pub stop_reason: StopReason,
}

impl ChatResponse {
    /// Returns the first text content, if any.
    pub fn text(&self) -> Option<&str> {
        self.content.iter().find_map(|p| match p {
            ContentPart::Text(t) => Some(t.as_str()),
            _ => None,
        })
    }

    /// Returns the first reasoning content, if any.
    pub fn reasoning(&self) -> Option<&str> {
        self.content.iter().find_map(|p| match p {
            ContentPart::Reasoning(t) => Some(t.as_str()),
            _ => None,
        })
    }

    /// Returns all tool-use parts from the response.
    pub fn tool_uses(&self) -> Vec<&ToolUse> {
        self.content
            .iter()
            .filter_map(|p| match p {
                ContentPart::ToolUse(t) => Some(t),
                _ => None,
            })
            .collect()
    }
}

/// Token usage information.
#[derive(Debug, Clone)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Tokens read from cache (prompt cache hit).
    pub cache_read_input_tokens: Option<u32>,
    /// Tokens written to cache (prompt cache miss / creation).
    pub cache_creation_input_tokens: Option<u32>,
}

/// Reason why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    Unknown(String),
}

/// An event from a streaming chat response.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of text content.
    ContentDelta(String),
    /// A chunk of reasoning content.
    ReasoningDelta(String),
    /// Start of a new tool call.
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    /// Incremental arguments JSON for a tool call.
    ToolCallDelta {
        index: usize,
        arguments_delta: String,
    },
    /// Token usage information.
    Usage(Usage),
    /// Generation finished.
    Done(StopReason),
}
