use std::io::Cursor;

use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessageContentPartImage,
        ChatCompletionRequestMessageContentPartText, ChatCompletionRequestUserMessageArgs,
        ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
        ChatCompletionTool, ChatCompletionToolChoiceOption, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, FunctionObject, ImageDetail, ImageUrl,
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

use crate::ensure_ends_with_punctuation;

#[derive(Debug, Serialize, Deserialize)]
pub struct AiMetadata {
    pub title_ru: String,
    pub slug: String,
    pub subtitle_ru: String,
    pub description_ru: String,
    pub text: Option<String>,
}

impl AiMetadata {
    pub fn from_meme_with_translation(
        meme: memes::Model,
        ru_translation: translations::Model,
    ) -> Self {
        Self {
            title_ru: ru_translation.title,
            slug: meme.slug,
            subtitle_ru: ru_translation.caption,
            description_ru: ru_translation.description,
            text: meme.text,
        }
    }

    pub fn apply(self, meme: &mut memes::ActiveModel, translation: &mut translations::ActiveModel) {
        meme.text = ActiveValue::set(self.text);
        meme.slug = ActiveValue::set(self.slug);

        translation.title = ActiveValue::set(self.title_ru);
        translation.caption = ActiveValue::set(self.subtitle_ru);
        translation.description = ActiveValue::set(self.description_ru);
    }
}

pub struct Ai {
    client: Client<OpenAIConfig>,
    http: reqwest::Client,
    jina_token: String,
}

fn save_metadata_tool() -> ChatCompletionTool {
    let save_func = FunctionObject {
        name: "save_meme_metadata".into(),
        description: Some("Save meme metadata".into()),
        parameters: Some(json!({
            "type": "object",
            "properties": {
                "title_ru": {
                    "type": "string",
                    "description": include_str!("../prompts/metadata/title.txt"),
                },
                "slug": {
                    "type": "string",
                    "description": include_str!("../prompts/metadata/slug.txt"),
                },
                "subtitle_ru": {
                    "type": "string",
                    "description": include_str!("../prompts/metadata/subtitle.txt"),
                },
                "description_ru": {
                    "type": "string",
                    "description": include_str!("../prompts/metadata/description.txt"),
                },
                "text": {
                    "type": "string",
                    "description": include_str!("../prompts/metadata/text.txt"),
                }
            },
            "required": [
                "title_ru",
                "slug",
                "subtitle_ru",
                "description_ru",
            ],
            "additionalProperties": false,
        })),

        strict: Some(false),
    };

    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: save_func,
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

    pub async fn gen_meme_metadata(&self, image: Vec<u8>) -> Result<AiMetadata> {
        let request = CreateChatCompletionRequestArgs::default()
            .model("gpt-4o-2024-11-20")
            .tools(vec![save_metadata_tool()])
            .max_tokens(1024u32)
            .tool_choice(ChatCompletionToolChoiceOption::Required)
            .messages(vec![
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content("Analyze provided meme and call function `save_meme_metadata`.\nAlways use double quotes (\") as quotation marks instead of signle (\').".to_string())
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(ChatCompletionRequestUserMessageContent::Array(vec![
                        image_to_messagepart(image),
                    ]))
                    .build()?
                    .into(),
            ])
            .build()?;

        let try_get_meta = async || {
            let response = self.client.chat().create(request.clone()).await?;
            let chat_choice = response.choices.into_iter().next().context("no choices")?;
            let tool_use = chat_choice
                .message
                .tool_calls
                .context("no tool calls")?
                .into_iter()
                .next()
                .context("no tool calls")?;
            let metadata: AiMetadata = from_str(&tool_use.function.arguments)?;
            Ok::<_, anyhow::Error>(metadata)
        };

        let mut last_error = None;
        for _ in 0..3 {
            let res = try_get_meta().await;
            if let Ok(metadata) = res {
                return Ok(metadata);
            } else if let Err(err) = res {
                last_error = Some(err);
            }
        }

        Err(last_error.unwrap())
    }

    pub async fn edit_meme_metadata(
        &self,
        ai_metadata: AiMetadata,
        image: Vec<u8>,
        edit_prompt: &str,
    ) -> Result<AiMetadata> {
        let request = CreateChatCompletionRequestArgs::default()
            .model("gpt-4o-2024-11-20")
            .tools(vec![save_metadata_tool()])
            .max_tokens(1024u32)
            .tool_choice(ChatCompletionToolChoiceOption::Required)
            .messages(vec![
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content("Apply edits from the user to current metadata of provided meme and update them via function `save_meme_metadata`.\nAlways use double quotes (\") as quotation marks instead of signle (\').".to_string())
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(ChatCompletionRequestUserMessageContent::Array(vec![
                        image_to_messagepart(image),
                        text_to_messagepart(format!("User's edits: ```{edit_prompt}```\n\nCurrent metadata:\n```{}```",
                        serde_json::to_string(&ai_metadata)?)),
                    ]))
                    .build()?
                    .into(),
            ])
            .build()?;

        let try_get_meta = async || {
            let response = self.client.chat().create(request.clone()).await?;
            let chat_choice = response.choices.into_iter().next().context("no choices")?;
            let tool_use = chat_choice
                .message
                .tool_calls
                .context("no tool calls")?
                .into_iter()
                .next()
                .context("no tool calls")?;
            let metadata: AiMetadata = from_str(&tool_use.function.arguments)?;
            Ok::<_, anyhow::Error>(metadata)
        };

        let mut last_error = None;
        for _ in 0..3 {
            let res = try_get_meta().await;
            if let Ok(metadata) = res {
                return Ok(metadata);
            } else if let Err(err) = res {
                last_error = Some(err);
            }
        }

        Err(last_error.unwrap())
    }
}
