use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{CreateEmbeddingRequestArgs, EmbeddingInput},
    Client,
};

pub struct OpenAi {
    client: Client<OpenAIConfig>,
}

impl OpenAi {
    pub fn new() -> Self {
        let client = Client::new();
        Self { client }
    }

    pub async fn embedding(&self, text: impl Into<String>) -> Result<Vec<f32>> {
        let request = CreateEmbeddingRequestArgs::default()
            .model("text-embedding-3-small")
            .input(EmbeddingInput::String(text.into()))
            .user("gdzach")
            .build()?;

        let response = self.client.embeddings().create(request).await?;

        Ok(response
            .data
            .into_iter()
            .next()
            .context("no data")?
            .embedding)
    }
}
