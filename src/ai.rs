use std::io::Cursor;

use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestMessageContentPartImage,
        ChatCompletionRequestMessageContentPartText, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionRequestUserMessageContent,
        ChatCompletionRequestUserMessageContentPart, CreateChatCompletionRequestArgs, ImageDetail,
        ImageUrl, ResponseFormat, ResponseFormatJsonSchema,
    },
    Client,
};
use base64::prelude::*;
use entities::{memes, translations};
use image::{codecs::jpeg::JpegEncoder, ImageReader};
use itertools::Itertools;
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json};
use tracing::info;

use crate::ensure_ends_with_punctuation;

#[derive(Debug, Serialize, Deserialize)]
pub struct AiMetadata {
    pub title: String,
    pub slug: String,
    pub subtitle: String,
    pub description: String,
    pub text_on_meme: Option<String>,
}

impl AiMetadata {
    pub fn from_meme_with_translation(
        meme: memes::Model,
        ru_translation: translations::Model,
    ) -> Self {
        Self {
            title: ru_translation.title,
            slug: meme.slug,
            subtitle: ru_translation.caption,
            description: ru_translation.description,
            text_on_meme: meme.text,
        }
    }

    pub fn apply(self, meme: &mut memes::ActiveModel, translation: &mut translations::ActiveModel) {
        meme.text = ActiveValue::set(self.text_on_meme);
        meme.slug = ActiveValue::set(self.slug);

        translation.title = ActiveValue::set(self.title);
        translation.caption = ActiveValue::set(self.subtitle);
        translation.description = ActiveValue::set(self.description);
    }
}

pub struct Ai {
    client: Client<OpenAIConfig>,
    http: reqwest::Client,
    jina_token: String,
}

fn response_format() -> ResponseFormat {
    ResponseFormat::JsonSchema {
        json_schema: ResponseFormatJsonSchema {
            description: Some("The content of the meme's web page".to_string()),
            name: "meme_info".to_string(),
            schema: Some(json!({
              "type": "object",
              "properties": {
                "title": {
                  "type": "string",
                  "description": "The title of the meme."
                },
                "subtitle": {
                  "type": "string",
                  "description": "The subtitle of the meme, also used as the alt tag."
                },
                "slug": {
                  "type": "string",
                  "description": "Slug in the URL."
                },
                "description": {
                  "type": "string",
                  "description": "Detailed description of the meme."
                },
                "text_on_meme": {
                  "type": ["string", "null"],
                  "description": "The text displayed on the meme image. Null if missing."
                }
              },
              "required": [
                "title",
                "subtitle",
                "slug",
                "description",
                "text_on_meme"
              ],
              "additionalProperties": false
            })),
            strict: Some(true),
        },
    }
}

fn image_to_messagepart(image: Vec<u8>) -> ChatCompletionRequestUserMessageContentPart {
    ChatCompletionRequestUserMessageContentPart::ImageUrl(
        ChatCompletionRequestMessageContentPartImage {
            image_url: ImageUrl {
                url: format!("data:image/jpeg;base64,{}", BASE64_STANDARD.encode(image)),
                detail: Some(ImageDetail::High),
            },
        },
    )
}

fn text_to_messagepart(text: String) -> ChatCompletionRequestUserMessageContentPart {
    ChatCompletionRequestUserMessageContentPart::Text(ChatCompletionRequestMessageContentPartText {
        text,
    })
}

#[derive(Deserialize)]
struct JinaAiResponse {
    data: Vec<JinaAiEmbedding>,
}

#[derive(Deserialize)]
struct JinaAiEmbedding {
    embedding: Vec<f32>,
}

impl Ai {
    pub fn new() -> Self {
        let client = Client::new();
        Self {
            client,
            http: reqwest::Client::new(),
            jina_token: std::env::var("JINA_API").expect("JINA_API must be provided"),
        }
    }

    pub async fn text_embedding(&self, text: impl Into<String>) -> Result<Vec<f32>> {
        let req = json!({
            "model": "jina-clip-v2",
            "dimensions": 1024,
            "task": "retrieval.query",
            "normalized": true,
            "embedding_type": "float",
            "input": [
                {
                    "text": text.into(),
                },
            ],
        });

        let res: JinaAiResponse = self
            .http
            .post("https://api.jina.ai/v1/embeddings")
            .json(&req)
            .bearer_auth(&self.jina_token)
            .send()
            .await?
            .json()
            .await?;

        res.data
            .into_iter()
            .map(|e| e.embedding)
            .next()
            .context("can't get result")
    }

    async fn jina_clip(&self, input: serde_json::Value) -> Result<Vec<Vec<f32>>> {
        let req = json!({
            "model": "jina-clip-v2",
            "dimensions": 1024,
            "normalized": true,
            "embedding_type": "float",
            "input": input,
        });

        let res: JinaAiResponse = self
            .http
            .post("https://api.jina.ai/v1/embeddings")
            .json(&req)
            .bearer_auth(&self.jina_token)
            .send()
            .await?
            .json()
            .await?;

        Ok(res.data.into_iter().map(|e| e.embedding).collect())
    }

    fn image_for_clip(thumb: &[u8]) -> Result<serde_json::Value> {
        let mut img = ImageReader::new(Cursor::new(thumb))
            .with_guessed_format()?
            .decode()?;

        if img.width() > 512 || img.height() > 512 {
            img = img.resize(512, 512, image::imageops::Lanczos3);
        }

        let mut img_bytes = Vec::new();
        let encoder = JpegEncoder::new_with_quality(&mut img_bytes, 90);
        img.write_with_encoder(encoder)?;

        Ok(json!({
            "image": BASE64_STANDARD.encode(img_bytes)
        }))
    }

    fn text_for_clip(
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<serde_json::Value> {
        let translation = translations.first().context("no translations")?;

        let mut text = format!(
            "Мем \"{}\".\n{}\n\n{}",
            translation.title,
            ensure_ends_with_punctuation(&translation.caption),
            translation.description
        );

        if let Some(text_on_meme) = &meme.text {
            text += "\n\nТекст: ";
            text += text_on_meme;
        }

        Ok(json!({
            "text": text,
        }))
    }

    pub async fn gen_meme_embedding(
        &self,
        meme: &memes::Model,
        thumb: &[u8],
        translations: &[translations::Model],
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        self.jina_clip(json!([
            Self::text_for_clip(meme, translations)?,
            Self::image_for_clip(thumb)?,
        ]))
        .await?
        .into_iter()
        .collect_tuple()
        .context("can't build 2-element tuple")
    }

    pub async fn get_image_embedding(&self, image: &[u8]) -> Result<Vec<f32>> {
        self.jina_clip(json!([Self::image_for_clip(image)?]))
            .await?
            .into_iter()
            .next()
            .context("no data")
    }

    pub async fn get_meme_text_embedding(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<Vec<f32>> {
        self.jina_clip(json!([Self::text_for_clip(meme, translations)?]))
            .await?
            .into_iter()
            .next()
            .context("no data")
    }

    async fn generate_ai_metadata(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        cheap_model: bool,
    ) -> Result<AiMetadata> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(if cheap_model {
                "gpt-4o-mini-2024-07-18"
            } else {
                "gpt-4o-2024-11-20"
            })
            .max_tokens(1024u32)
            .response_format(response_format())
            .messages(messages)
            .build()?;

        let response = self.client.chat().create(request).await?;
        let usage = response.usage.context("no usage")?;
        info!(
            "done generating metadata, usage: {} in, {} out",
            usage.prompt_tokens, usage.completion_tokens
        );
        let message = response
            .choices
            .into_iter()
            .next()
            .context("no choices")?
            .message
            .content
            .context("no message")?;
        Ok(from_str(&message)?)
    }

    pub async fn gen_new_meme_metadata(
        &self,
        image: Vec<u8>,
        cheap_model: bool,
    ) -> Result<AiMetadata> {
        self.generate_ai_metadata(
            vec![
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(include_str!("../prompts/meta.md"))
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(ChatCompletionRequestUserMessageContent::Array(vec![
                        image_to_messagepart(image),
                    ]))
                    .build()?
                    .into(),
            ],
            cheap_model,
        )
        .await
    }

    pub async fn generate_edited_meme_metadata(
        &self,
        ai_metadata: AiMetadata,
        image: Vec<u8>,
        edit_prompt: &str,
    ) -> Result<AiMetadata> {
        self.generate_ai_metadata(vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(include_str!("../prompts/meta.md"))
                .build()?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(ChatCompletionRequestUserMessageContent::Array(vec![
                    image_to_messagepart(image),
                    text_to_messagepart(format!(
                        "Update existing page content according to the user feedback: ```{edit_prompt}```\n\nCurrent content:\n```{}```",
                        serde_json::to_string(&ai_metadata)?
                    )),
                ]))
                .build()?
                .into(),
        ], false)
        .await
    }
}
