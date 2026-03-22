use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::client::Client;

static LLM_CONFIG_ID: AtomicI64 = AtomicI64::new(1);
static LLM_PROVIDER_CONFIG_ID: AtomicI64 = AtomicI64::new(1);

/// Provider API type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderType {
    OpenAi,
    Anthropic,
}

/// Configuration for a specific LLM model.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub id: i64,
    pub model_name: String,
    pub model_id: String,
    pub max_output_token: i32,
    pub max_input_token: i32,
    pub max_token: i32,
    pub thinking: bool,
    pub image_understanding: bool,
    pub struct_output: bool,
    pub tool_use: bool,
    pub temperature: f32,
    pub top_p: f32,
}

/// Configuration for an LLM provider (vendor endpoint).
#[derive(Debug, Clone)]
pub struct LlmProviderConfig {
    pub id: i64,
    pub provider_name: String,
    pub base_url: String,
    pub provider_type: ProviderType,
    pub api_key: String,
    pub custom_header: HashMap<String, String>,
}

impl LlmConfig {
    pub fn new(
        model_name: impl Into<String>,
        model_id: impl Into<String>,
        max_output_token: i32,
        max_input_token: i32,
        max_token: i32,
    ) -> Self {
        Self {
            id: LLM_CONFIG_ID.fetch_add(1, Ordering::Relaxed),
            model_name: model_name.into(),
            model_id: model_id.into(),
            max_output_token,
            max_input_token,
            max_token,
            thinking: false,
            image_understanding: false,
            struct_output: false,
            tool_use: false,
            temperature: 1.0,
            top_p: 1.0,
        }
    }

    pub fn thinking(mut self, v: bool) -> Self {
        self.thinking = v;
        self
    }

    pub fn image_understanding(mut self, v: bool) -> Self {
        self.image_understanding = v;
        self
    }

    pub fn struct_output(mut self, v: bool) -> Self {
        self.struct_output = v;
        self
    }

    pub fn tool_use(mut self, v: bool) -> Self {
        self.tool_use = v;
        self
    }

    pub fn temperature(mut self, v: f32) -> Self {
        self.temperature = v;
        self
    }

    pub fn top_p(mut self, v: f32) -> Self {
        self.top_p = v;
        self
    }
}

impl LlmProviderConfig {
    pub fn new(
        provider_name: impl Into<String>,
        base_url: impl Into<String>,
        provider_type: ProviderType,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            id: LLM_PROVIDER_CONFIG_ID.fetch_add(1, Ordering::Relaxed),
            provider_name: provider_name.into(),
            base_url: base_url.into(),
            provider_type,
            api_key: api_key.into(),
            custom_header: HashMap::new(),
        }
    }

    pub fn custom_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom_header.insert(key.into(), value.into());
        self
    }

    pub fn custom_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.custom_header = headers;
        self
    }

    /// Build a [`Client`] from this provider configuration.
    pub fn build_client(&self) -> Client {
        Client::from_provider(self)
    }
}
