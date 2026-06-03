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
