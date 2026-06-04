//! A deterministic, dependency-free [`ModelProvider`] for the walking skeleton.
//!
//! It performs no real inference: it deterministically extracts a candidate
//! "answer entity" from the prompt so the end-to-end loop (including
//! self-consistency, which calls the model multiple times) runs reproducibly
//! without a model server. Swap in `async-openai` (on-prem vLLM/TGI) behind the
//! same [`ModelProvider`] trait — see `docs/05-tech-decisions.md`.

use async_trait::async_trait;
use oncora_core::{ModelPin, ModelProvider, Result};

pub struct TemplateModel {
    pin: ModelPin,
}

impl Default for TemplateModel {
    fn default() -> Self {
        Self {
            pin: ModelPin::new("stub/template-model", "v0"),
        }
    }
}

#[async_trait]
impl ModelProvider for TemplateModel {
    fn model_pin(&self) -> ModelPin {
        self.pin.clone()
    }

    async fn complete(&self, prompt: &str) -> Result<String> {
        // Deterministic "extraction": the longest capitalized alphanumeric
        // token in the prompt (a stand-in for an extracted target/entity),
        // ignoring the structural prompt markers.
        const SKIP: [&str; 3] = ["CONTEXT", "QUESTION", "ANSWER"];
        let pick = prompt
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| {
                t.chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
            })
            .filter(|t| !SKIP.contains(t))
            .max_by_key(|t| t.len())
            .unwrap_or("uncertain");
        Ok(pick.to_string())
    }
}

/// A real [`ModelProvider`] backed by any OpenAI-compatible endpoint
/// (Ollama, vLLM, TGI). Requests use `temperature = 0` so repeated calls are
/// deterministic — self-consistency stays meaningful and runs stay replayable.
#[cfg(feature = "openai")]
pub use openai::{OpenAiEmbedder, OpenAiModel};

#[cfg(feature = "openai")]
mod openai {
    use async_openai::Client;
    use async_openai::config::OpenAIConfig;
    use async_openai::types::chat::{
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    };
    use async_openai::types::embeddings::CreateEmbeddingRequestArgs;
    use async_trait::async_trait;
    use oncora_core::{EmbeddingProvider, ModelPin, ModelProvider, OncoraError, Result};

    /// Talks to an OpenAI-compatible chat-completions API.
    pub struct OpenAiModel {
        client: Client<OpenAIConfig>,
        model: String,
        pin: ModelPin,
        max_tokens: u32,
    }

    fn provider(e: impl std::fmt::Display) -> OncoraError {
        OncoraError::Provider(format!("openai: {e}"))
    }

    impl OpenAiModel {
        /// Connect to `base_url` (e.g. `http://127.0.0.1:11434/v1` for Ollama)
        /// serving `model`. `api_key` may be a placeholder for local servers.
        pub fn new(base_url: &str, api_key: &str, model: &str) -> Self {
            let config = OpenAIConfig::new()
                .with_api_base(base_url)
                .with_api_key(api_key);
            Self {
                client: Client::with_config(config),
                model: model.to_string(),
                pin: ModelPin::new(format!("openai/{model}"), "live"),
                max_tokens: 64,
            }
        }

        /// Build from `ONCORA_OPENAI_URL` (+ optional `ONCORA_OPENAI_KEY`,
        /// `ONCORA_OPENAI_MODEL`); returns `None` if the URL is unset.
        pub fn from_env() -> Option<Self> {
            let url = std::env::var("ONCORA_OPENAI_URL").ok()?;
            let key = std::env::var("ONCORA_OPENAI_KEY").unwrap_or_else(|_| "local".to_string());
            let model =
                std::env::var("ONCORA_OPENAI_MODEL").unwrap_or_else(|_| "qwen2.5:0.5b".to_string());
            Some(Self::new(&url, &key, &model))
        }
    }

    #[async_trait]
    impl ModelProvider for OpenAiModel {
        fn model_pin(&self) -> ModelPin {
            self.pin.clone()
        }

        async fn complete(&self, prompt: &str) -> Result<String> {
            let message = ChatCompletionRequestUserMessageArgs::default()
                .content(prompt)
                .build()
                .map_err(provider)?;
            let request = CreateChatCompletionRequestArgs::default()
                .model(&self.model)
                .temperature(0.0) // deterministic -> reproducible
                .max_tokens(self.max_tokens)
                .messages([message.into()])
                .build()
                .map_err(provider)?;
            let response = self.client.chat().create(request).await.map_err(provider)?;
            Ok(response
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .unwrap_or_default()
                .trim()
                .to_string())
        }
    }

    /// A real [`EmbeddingProvider`] backed by the same OpenAI-compatible
    /// endpoint (e.g. Ollama serving `all-minilm` / `nomic-embed-text`).
    pub struct OpenAiEmbedder {
        client: Client<OpenAIConfig>,
        model: String,
        dims: usize,
    }

    impl OpenAiEmbedder {
        /// Connect and probe the embedding dimensionality once (so callers can
        /// size a vector collection to match).
        pub async fn connect(base_url: &str, api_key: &str, model: &str) -> Result<Self> {
            let config = OpenAIConfig::new()
                .with_api_base(base_url)
                .with_api_key(api_key);
            let mut me = Self {
                client: Client::with_config(config),
                model: model.to_string(),
                dims: 0,
            };
            let probe = me.embed(&["dimension probe".to_string()]).await?;
            me.dims = probe.first().map(|v| v.len()).unwrap_or(0);
            if me.dims == 0 {
                return Err(OncoraError::Provider("openai: empty embedding".into()));
            }
            Ok(me)
        }

        /// Build from `ONCORA_OPENAI_URL` (+ optional `ONCORA_OPENAI_KEY`,
        /// `ONCORA_EMBED_MODEL`); returns `None` if the URL is unset.
        pub async fn from_env() -> Option<Result<Self>> {
            let url = std::env::var("ONCORA_OPENAI_URL").ok()?;
            let key = std::env::var("ONCORA_OPENAI_KEY").unwrap_or_else(|_| "local".to_string());
            let model =
                std::env::var("ONCORA_EMBED_MODEL").unwrap_or_else(|_| "all-minilm".to_string());
            Some(Self::connect(&url, &key, &model).await)
        }
    }

    #[async_trait]
    impl EmbeddingProvider for OpenAiEmbedder {
        fn dims(&self) -> usize {
            self.dims
        }

        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let request = CreateEmbeddingRequestArgs::default()
                .model(&self.model)
                .input(texts.to_vec())
                .build()
                .map_err(provider)?;
            let response = self
                .client
                .embeddings()
                .create(request)
                .await
                .map_err(provider)?;
            Ok(response.data.into_iter().map(|e| e.embedding).collect())
        }
    }
}
