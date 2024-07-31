use anyhow::{bail, Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessageContentPart,
        ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestUserMessageArgs,
        ChatCompletionRequestUserMessageContent, ChatCompletionTool,
        ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        CreateEmbeddingRequestArgs, EmbeddingInput, FunctionObject, ImageDetail, ImageUrl,
    },
    Client,
};
use base64::prelude::*;
use serde::Deserialize;
use serde_json::{from_str, json};

#[derive(Deserialize)]
pub struct AiMetadata {
    pub title_ru: String,
    pub slug: String,
    pub subtitle_ru: String,
    pub description_ru: String,
    pub fixed_text: Option<String>,
}

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

    pub async fn gen_meme_metadata(&self, image: Vec<u8>) -> Result<AiMetadata> {
        let save_function = FunctionObject {
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
                ]
            })),
        };
        let save_tool = ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: save_function,
        };

        let request = CreateChatCompletionRequestArgs::default()
            .model("gpt-4o")
            .tools(vec![save_tool])
            .max_tokens(1024u32)
            .tool_choice(ChatCompletionToolChoiceOption::Required)
            .messages(vec![
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content("Analyze provided meme and call function `save_meme_metadata`. \
                    The title should be a short, succinct, concise phrase, begin with a capital letter and not end with a period. \
                    The slug must be a translation of the title into English and consist only of Latin letters and hyphens. \
                    The subtitle should be a small capitalized sentence without a period that complements the title. \
                    The description should be long and detailed, describing what is shown in the picture and explaining what the meme is about. \
                    If the text in the picture is present, you need to correct the capslock (and capitalize the first letter) and divide it into sentences. Add the end of the sentence if there is none. \
                    The title, subtitle and descriptions must be written in Russian.\
                    Use double quotes (\") as quotation marks.")
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(ChatCompletionRequestUserMessageContent::Array(vec![
                        ChatCompletionRequestMessageContentPart::ImageUrl(
                            ChatCompletionRequestMessageContentPartImage {
                                image_url: ImageUrl {
                                    url: format!(
                                        "data:image/jpeg;base64,{}",
                                        BASE64_STANDARD.encode(image)
                                    ),
                                    detail: Some(ImageDetail::High),
                                },
                            },
                        ),
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

        for _ in 0..3 {
            if let Ok(metadata) = try_get_meta().await {
                return Ok(metadata);
            }
        }

        bail!("all attempts failed");
    }
}
