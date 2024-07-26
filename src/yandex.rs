use std::env;

use anyhow::Result;
use base64::prelude::*;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::{Deserialize, Serialize};

pub struct Yandex {
    ycl_api_key: String,
    client: ClientWithMiddleware,
}

impl Yandex {
    pub fn new() -> Result<Self> {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Ok(Self {
            ycl_api_key: env::var("YCL_API_KEY")?,
            client,
        })
    }

    pub async fn ocr(&self, image: Vec<u8>) -> Result<String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RecognitionRequest {
            mime_type: String,
            language_codes: Vec<String>,
            model: String,
            content: String,
        }

        #[derive(Deserialize)]
        struct RecognitionResponse {
            result: RecognitionResponseResult,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RecognitionResponseResult {
            text_annotation: TextAnnotation,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TextAnnotation {
            full_text: String,
        }

        let res: RecognitionResponse = self
            .client
            .post("https://ocr.api.cloud.yandex.net/ocr/v1/recognizeText")
            .header("Authorization", format!("Api-Key {}", self.ycl_api_key))
            .json(&RecognitionRequest {
                mime_type: "JPEG".into(),
                language_codes: vec!["en".into(), "ru".into()],
                model: "page".into(),
                content: BASE64_STANDARD.encode(&image),
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(res.result.text_annotation.full_text)
    }
}
