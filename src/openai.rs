use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessageContentPartImage,
        ChatCompletionRequestMessageContentPartText, ChatCompletionRequestUserMessageArgs,
        ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
        ChatCompletionTool, ChatCompletionToolChoiceOption, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, CreateEmbeddingRequestArgs, EmbeddingInput,
        FunctionObject, ImageDetail, ImageUrl,
    },
    Client,
};
use base64::prelude::*;
use entities::{memes, translations};
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json};

use crate::ensure_ends_with_punctuation;

const COMMON_PROMPT: &str = "The title should be a short, succinct, concise phrase, begin with a capital letter and not end with a period.
The slug must be a translation of the title into English and consist only of Latin letters and hyphens.
The subtitle should be a small capitalized sentence without a period that complements the title.
The description should be long and detailed, describing what is shown in the picture and explaining what the meme is about.
If the text in the picture is present, you need to correct the capslock (and capitalize the first letter) and divide it into sentences. Add the end of the sentence if there is none.
The title, subtitle and descriptions must be written in Russian.
Always use double quotes (\") as quotation marks instead of signle (\').";

#[derive(Debug, Serialize, Deserialize)]
pub struct AiMetadata {
    pub title_ru: String,
    pub slug: String,
    pub subtitle_ru: String,
    pub description_ru: String,
    pub fixed_text: Option<String>,
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
            fixed_text: meme.text,
        }
    }

    pub fn apply(self, meme: &mut memes::ActiveModel, translation: &mut translations::ActiveModel) {
        meme.text = ActiveValue::set(self.fixed_text);
        meme.slug = ActiveValue::set(self.slug);

        translation.title = ActiveValue::set(self.title_ru);
        translation.caption = ActiveValue::set(self.subtitle_ru);
        translation.description = ActiveValue::set(self.description_ru);
    }
}

pub struct OpenAi {
    client: Client<OpenAIConfig>,
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
                    "description": "Laconic and short title in Russian language."
                },
                "slug": {
                    "type": "string",
                    "description": "Slug. Part of meme's url address."
                },
                "subtitle_ru": {
                    "type": "string",
                    "description": "Subtitle in Russian language."
                },
                "description_ru": {
                    "type": "string",
                    "description": "Very long and detailed description of the meme in Russian language."
                },
                "fixed_text": {
                    "type": "string",
                    "description": "The text in the picture, divided into sentences and with corrected registers."
                }
            },
            "required": [
                "title_ru",
                "slug",
                "subtitle_ru",
                "description_ru"
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

impl OpenAi {
    pub fn new() -> Self {
        let client = Client::new();
        Self { client }
    }

    pub async fn text_embedding(&self, text: impl Into<String>) -> Result<Vec<f32>> {
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

    pub async fn gen_meme_text_embedding(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<Vec<f32>> {
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

        self.text_embedding(text).await
    }

    pub async fn gen_meme_metadata(&self, image: Vec<u8>) -> Result<AiMetadata> {
        let request = CreateChatCompletionRequestArgs::default()
            .model("gpt-4o")
            .tools(vec![save_metadata_tool()])
            .max_tokens(1024u32)
            .tool_choice(ChatCompletionToolChoiceOption::Required)
            .messages(vec![
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(format!("Analyze provided meme and call function `save_meme_metadata`.\n\n{COMMON_PROMPT}"))
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
            .model("gpt-4o")
            .tools(vec![save_metadata_tool()])
            .max_tokens(1024u32)
            .tool_choice(ChatCompletionToolChoiceOption::Required)
            .messages(vec![
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(format!("Apply edits from the user to current metadata and update them via function `save_meme_metadata`.\n\n{COMMON_PROMPT}"))
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(ChatCompletionRequestUserMessageContent::Array(vec![
                        image_to_messagepart(image),
                        text_to_messagepart(format!("User's edits: {edit_prompt}.\n\nCurrent metadata:\nTitle: {}.\nSlug: {}.\nSubtitle: {}.\nDescription: {}.\nText: {}.",
                        ai_metadata.title_ru, ai_metadata.slug, ai_metadata.subtitle_ru, ai_metadata.description_ru, ai_metadata.fixed_text.unwrap_or_else(|| "none".to_string()))),
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
