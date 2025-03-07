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
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json, Value};
use tracing::info;

use crate::ensure_ends_with_punctuation;

#[derive(Debug, Serialize, Deserialize)]
pub struct AiMetadata {
    pub title: String,
    pub slug: String,
    pub subtitle: String,
    pub description: String,
    pub text_on_meme: String,
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
            text_on_meme: meme.text.unwrap_or_default(),
        }
    }

    pub fn apply(self, meme: &mut memes::ActiveModel, translation: &mut translations::ActiveModel) {
        meme.text = ActiveValue::set(if self.text_on_meme.is_empty() {
            None
        } else {
            Some(self.text_on_meme)
        });
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
                  "type": "string",
                  "description": "The text displayed on the meme image. Empty if missing."
                }
              },
              "required": [
                "title",
                "subtitle",
                "slug",
                "description",
                "text_on_meme"
              ],
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

pub enum JinaClipInput {
    Image(Vec<u8>),
    Text(String),
}

impl From<Vec<u8>> for JinaClipInput {
    fn from(value: Vec<u8>) -> Self {
        Self::Image(value)
    }
}

impl From<String> for JinaClipInput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

pub enum JinaTaskType {
    Passage,
    Query,
}

impl TryFrom<JinaClipInput> for Value {
    type Error = anyhow::Error;

    fn try_from(value: JinaClipInput) -> Result<Self> {
        match value {
            JinaClipInput::Image(img) => {
                let mut img = ImageReader::new(Cursor::new(img))
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
            JinaClipInput::Text(txt) => Ok(json!({
                "text": txt,
            })),
        }
    }
}

#[derive(Serialize)]
struct JinaAiClipRequest {
    model: String,
    dimensions: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<String>,
    normalized: bool,
    embedding_type: String,
    input: (Value,),
}

#[derive(Serialize)]
struct JinaAiTextRequest {
    model: String,
    task: String,
    late_chunking: bool,
    dimensions: u32,
    embedding_type: String,
    input: (Value,),
}

#[derive(Deserialize)]
struct JinaAiResponse {
    data: (JinaAiEmbedding,),
}

#[derive(Deserialize)]
struct JinaAiEmbedding {
    embedding: Vec<f32>,
}

impl Ai {
    pub fn new() -> Self {
        let client = Client::with_config(
            OpenAIConfig::new()
                .with_api_base("https://generativelanguage.googleapis.com/v1beta/openai")
                .with_api_key(std::env::var("GEMINI_API_KEY").expect("JINA_API must be provided")),
        );
        Self {
            client,
            http: reqwest::Client::new(),
            jina_token: std::env::var("JINA_API").expect("JINA_API must be provided"),
        }
    }

    pub async fn jina_clip(&self, input: JinaClipInput, task: JinaTaskType) -> Result<Vec<f32>> {
        let task = match task {
            JinaTaskType::Passage => None,
            JinaTaskType::Query => Some("retrieval.query".to_string()),
        };

        let req = JinaAiClipRequest {
            model: "jina-clip-v2".into(),
            dimensions: 1024,
            task,
            normalized: true,
            embedding_type: "float".into(),
            input: (input.try_into()?,),
        };

        self.get_jina_embeddings(req).await
    }

    pub async fn jina_text(&self, input: &str, task: JinaTaskType) -> Result<Vec<f32>> {
        let task = match task {
            JinaTaskType::Passage => "retrieval.passage",
            JinaTaskType::Query => "retrieval.query",
        };

        let req = JinaAiTextRequest {
            model: "jina-embeddings-v3".into(),
            task: task.into(),
            late_chunking: true,
            dimensions: 1024,
            embedding_type: "float".into(),
            input: (input.into(),),
        };

        self.get_jina_embeddings(req).await
    }

    async fn get_jina_embeddings(&self, req: impl Serialize) -> Result<Vec<f32>> {
        let res: JinaAiResponse = self
            .http
            .post("https://api.jina.ai/v1/embeddings")
            .json(&req)
            .bearer_auth(&self.jina_token)
            .send()
            .await?
            .json()
            .await?;

        Ok(res.data.0.embedding)
    }

    pub fn get_text_for_embedding(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Option<String> {
        let translation = translations.first()?;

        let mut text = format!(
            "# {}\n{}\n\n{}",
            translation.title,
            ensure_ends_with_punctuation(&translation.caption),
            translation.description
        );

        if let Some(text_on_meme) = &meme.text {
            text += "\n\nТекст:\n";
            text += text_on_meme;
        }

        Some(text)
    }

    async fn generate_ai_metadata(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        _cheap_model: bool,
    ) -> Result<AiMetadata> {
        let request = CreateChatCompletionRequestArgs::default()
            .model("gemini-2.0-flash")
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
