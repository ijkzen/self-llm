use std::collections::HashMap;
use std::pin::Pin;

use futures::Stream;

use crate::{
    config::{LlmProviderConfig, ProviderType},
    error::Error,
    types::{ChatRequest, ChatResponse, StreamEvent},
};

fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .expect("failed to build HTTP client")
}

/// Unified LLM client that works with both OpenAI and Anthropic APIs.
pub struct Client {
    provider: Provider,
    http: reqwest::Client,
}

enum Provider {
    OpenAi {
        api_key: String,
        base_url: String,
        custom_headers: HashMap<String, String>,
    },
    Anthropic {
        api_key: String,
        base_url: String,
        api_version: String,
        custom_headers: HashMap<String, String>,
    },
}

impl Client {
    /// Create a client for the OpenAI API (`https://api.openai.com/v1`).
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self::openai_with_base_url(api_key, "https://api.openai.com/v1")
    }

    /// Create a client for an OpenAI-compatible API with a custom base URL.
    pub fn openai_with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            provider: Provider::OpenAi {
                api_key: api_key.into(),
                base_url: base_url.into(),
                custom_headers: HashMap::new(),
            },
            http: build_http_client(),
        }
    }

    /// Create a client for the Anthropic API (`https://api.anthropic.com`).
    pub fn anthropic(api_key: impl Into<String>) -> Self {
        Self::anthropic_with_base_url(api_key, "https://api.anthropic.com")
    }

    /// Create a client for an Anthropic-compatible API with a custom base URL.
    pub fn anthropic_with_base_url(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            provider: Provider::Anthropic {
                api_key: api_key.into(),
                base_url: base_url.into(),
                api_version: "2023-06-01".to_string(),
                custom_headers: HashMap::new(),
            },
            http: build_http_client(),
        }
    }

    /// Create a client from a [`LlmProviderConfig`].
    pub fn from_provider(config: &LlmProviderConfig) -> Self {
        let http = build_http_client();
        match config.provider_type {
            ProviderType::OpenAi => Self {
                provider: Provider::OpenAi {
                    api_key: config.api_key.clone(),
                    base_url: config.base_url.clone(),
                    custom_headers: config.custom_header.clone(),
                },
                http,
            },
            ProviderType::Anthropic => Self {
                provider: Provider::Anthropic {
                    api_key: config.api_key.clone(),
                    base_url: config.base_url.clone(),
                    api_version: "2023-06-01".to_string(),
                    custom_headers: config.custom_header.clone(),
                },
                http,
            },
        }
    }

    /// Send a chat completion request and wait for the full response.
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, Error> {
        match &self.provider {
            Provider::OpenAi {
                api_key,
                base_url,
                custom_headers,
            } => crate::openai::chat(&self.http, base_url, api_key, custom_headers, request).await,
            Provider::Anthropic {
                api_key,
                base_url,
                api_version,
                custom_headers,
            } => {
                crate::anthropic::chat(
                    &self.http,
                    base_url,
                    api_key,
                    api_version,
                    custom_headers,
                    request,
                )
                .await
            }
        }
    }

    /// Send a chat completion request and receive a stream of incremental events.
    pub async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, Error>> + Send>>, Error> {
        match &self.provider {
            Provider::OpenAi {
                api_key,
                base_url,
                custom_headers,
            } => {
                crate::openai::chat_stream(&self.http, base_url, api_key, custom_headers, request)
                    .await
            }
            Provider::Anthropic {
                api_key,
                base_url,
                api_version,
                custom_headers,
            } => {
                crate::anthropic::chat_stream(
                    &self.http,
                    base_url,
                    api_key,
                    api_version,
                    custom_headers,
                    request,
                )
                .await
            }
        }
    }
}
