use std::env;

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct Yandex {
    ycl_api_key: String,
    ycl_folder: String,
    client: Client,
}

impl Yandex {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ycl_api_key: env::var("YCL_API_KEY")?,
            ycl_folder: env::var("YCL_FOLDER")?,
            client: Client::new(),
        })
    }

    pub async fn text_embedding(
        &self,
        text: impl Into<String>,
        model_type: &str,
    ) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct EmbeddingRequest {
            model_uri: String,
            text: String,
        }

        #[derive(Deserialize)]
        struct EmbeddingResponse {
            embedding: Vec<f32>,
        }

        let model_uri = format!("emb://{}/{model_type}/latest", self.ycl_folder);

        let res: EmbeddingResponse = self
            .client
            .post("https://llm.api.cloud.yandex.net/foundationModels/v1/textEmbedding")
            .header("Authorization", format!("Api-Key {}", self.ycl_api_key))
            .json(&EmbeddingRequest {
                text: text.into(),
                model_uri,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(res.embedding)
    }
}
