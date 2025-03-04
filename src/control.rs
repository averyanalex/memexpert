use std::{fmt::Display, fmt::Write, str::FromStr};

use anyhow::{bail, Context, Result};
use entities::{
    memes,
    sea_orm_active_enums::{MediaType, PublishStatus},
    translations,
};
use teloxide::{
    prelude::*,
    types::{
        InlineKeyboardButton, InlineKeyboardMarkup, InputFile, InputMedia, InputMediaAnimation,
        InputMediaPhoto, InputMediaVideo, MessageId,
    },
};

use crate::{bot::Bot, ensure_ends_with_punctuation};

#[derive(Clone)]
pub enum MemeEditAction {
    Ai,
    Slug,
    Title,
    Caption,
    Description,
    Text,
    Source,
    Publish,
    Draft,
    Trash,
    File,
}

impl Display for MemeEditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char(match self {
            Self::Ai => 'a',
            Self::Title => 't',
            Self::Slug => 's',
            Self::Caption => 'c',
            Self::Description => 'd',
            Self::Text => 'e',
            Self::Source => 'm',
            Self::Publish => 'p',
            Self::Draft => 'r',
            Self::Trash => 'h',
            Self::File => 'f',
        })
    }
}

impl MemeEditAction {
    fn from_char(char: char) -> Result<Self> {
        Ok(match char {
            'a' => Self::Ai,
            't' => Self::Title,
            's' => Self::Slug,
            'c' => Self::Caption,
            'd' => Self::Description,
            'e' => Self::Text,
            'm' => Self::Source,
            'p' => Self::Publish,
            'r' => Self::Draft,
            'h' => Self::Trash,
            'f' => Self::File,
            _ => bail!("unknown char: {char}"),
        })
    }
}

pub struct MemeEditCallback {
    pub action: MemeEditAction,
    pub meme_id: i32,
    pub language: String,
}

impl Display for MemeEditCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}{}", self.action, self.language, self.meme_id)
    }
}

impl FromStr for MemeEditCallback {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let mut chars = s.chars();
        let action = MemeEditAction::from_char(chars.next().context("no chars")?)?;
        let language: String = chars.clone().take(2).collect();
        let meme_id = chars.skip(2).collect::<String>().parse()?;
        Ok(Self {
            action,
            meme_id,
            language,
        })
    }
}

fn gen_meme_control_text(meme: &memes::Model, translations: &[translations::Model]) -> String {
    let lang = &translations[0].language;
    let mut t = format!(
        "URL: https://memexpert.net/{lang}/{}.\nИсточник: {}.",
        meme.slug,
        meme.source.as_ref().map_or("неизвестен", |t| t.as_str())
    );

    for translation in translations {
        write!(
            t,
            "\n\n[{}] {}\nПодпись: {}\nОписание: {}",
            translation.language.to_uppercase(),
            ensure_ends_with_punctuation(&translation.title),
            ensure_ends_with_punctuation(&translation.caption),
            translation.description
        )
        .unwrap();
    }

    write!(
        t,
        "\n\nТекст: {}",
        meme.text.as_ref().map_or("отсутствует.", |t| t.as_str()),
    )
    .unwrap();

    write!(
        t,
        "\n\nСоздан {} в {}, изменён {} в {}",
        meme.created_by, meme.creation_time, meme.last_edited_by, meme.last_edition_time,
    )
    .unwrap();

    if t.len() > 1024 {
        t = t.chars().take(1024).collect();
    }

    t
}

fn gen_meme_control_keyboard(
    meme: &memes::Model,
    translations: &[translations::Model],
) -> InlineKeyboardMarkup {
    let gen_publish_status_text = |status: PublishStatus| {
        let emoji = match status {
            PublishStatus::Draft => '📝',
            PublishStatus::Published => '🌐',
            PublishStatus::Trash => '🗑',
        };
        if meme.publish_status == status {
            format!("[{emoji}]")
        } else {
            emoji.to_string()
        }
    };

    InlineKeyboardMarkup::new(
        [
            vec![
                InlineKeyboardButton::callback(
                    "Слаг",
                    MemeEditCallback {
                        action: MemeEditAction::Slug,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    "Текст",
                    MemeEditCallback {
                        action: MemeEditAction::Text,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    "Источник",
                    MemeEditCallback {
                        action: MemeEditAction::Source,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    "Файл",
                    MemeEditCallback {
                        action: MemeEditAction::File,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    "AI",
                    MemeEditCallback {
                        action: MemeEditAction::Ai,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
            ],
            vec![
                InlineKeyboardButton::callback(
                    gen_publish_status_text(PublishStatus::Published),
                    MemeEditCallback {
                        action: MemeEditAction::Publish,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    gen_publish_status_text(PublishStatus::Draft),
                    MemeEditCallback {
                        action: MemeEditAction::Draft,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    gen_publish_status_text(PublishStatus::Trash),
                    MemeEditCallback {
                        action: MemeEditAction::Trash,
                        meme_id: meme.id,
                        language: "  ".to_owned(),
                    }
                    .to_string(),
                ),
            ],
        ]
        .into_iter()
        .chain(translations.iter().map(|translation| {
            vec![
                InlineKeyboardButton::callback(
                    format!("[{}] Название", translation.language.to_uppercase()),
                    MemeEditCallback {
                        action: MemeEditAction::Title,
                        meme_id: meme.id,
                        language: translation.language.clone(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    "Подпись",
                    MemeEditCallback {
                        action: MemeEditAction::Caption,
                        meme_id: meme.id,
                        language: translation.language.clone(),
                    }
                    .to_string(),
                ),
                InlineKeyboardButton::callback(
                    "Описание",
                    MemeEditCallback {
                        action: MemeEditAction::Description,
                        meme_id: meme.id,
                        language: translation.language.clone(),
                    }
                    .to_string(),
                ),
            ]
        })),
    )
}

/// Update or create meme control message in admin channel.
pub async fn refresh_meme_control_msg(
    bot: &Bot,
    meme: &memes::Model,
    translations: &[translations::Model],
) -> Result<Option<Message>> {
    let text = gen_meme_control_text(meme, translations);
    let keyboard = gen_meme_control_keyboard(meme, translations);

    let chat_id: i64 = std::env::var("ADMIN_CHANNEL_ID")?.parse()?;
    let chat_id = ChatId(chat_id);
    let input_file = InputFile::file_id(meme.tg_id.clone());

    Ok(if meme.control_message_id == -1 {
        Some(match meme.media_type {
            MediaType::Animation => {
                bot.send_animation(chat_id, input_file)
                    .caption(text)
                    .reply_markup(keyboard)
                    .await?
            }
            MediaType::Photo => {
                bot.send_photo(chat_id, input_file)
                    .caption(text)
                    .reply_markup(keyboard)
                    .await?
            }
            MediaType::Video => {
                bot.send_video(chat_id, input_file)
                    .caption(text)
                    .reply_markup(keyboard)
                    .await?
            }
        })
    } else {
        let msg_id = MessageId(meme.control_message_id);
        let input_media = match meme.media_type {
            MediaType::Animation => InputMedia::Animation(InputMediaAnimation::new(input_file)),
            MediaType::Photo => InputMedia::Photo(InputMediaPhoto::new(input_file)),
            MediaType::Video => InputMedia::Video(InputMediaVideo::new(input_file)),
        };
        bot.edit_message_media(chat_id, msg_id, input_media).await?;
        bot.edit_message_caption(chat_id, msg_id)
            .caption(text)
            .reply_markup(keyboard)
            .await?;

        None
    })
}
