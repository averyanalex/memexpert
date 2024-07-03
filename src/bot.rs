use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{bail, Context, Result};
use entities::{
    memes,
    sea_orm_active_enums::{MediaType, PublishStatus},
    translations,
};
use itertools::Itertools;
use sea_orm::{ActiveModelBehavior, ActiveValue};
use teloxide::{
    prelude::*,
    types::{
        FileMeta, InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult,
        InlineQueryResultCachedGif, InlineQueryResultCachedPhoto, InlineQueryResultCachedVideo,
        KeyboardButton, KeyboardMarkup, KeyboardRemove, ParseMode,
    },
};
use translit::ToLatin;

use crate::{
    control::{MemeEditAction, MemeEditCallback},
    storage::Storage,
};

pub async fn run_bot(db: Storage, bot: Bot) -> Result<()> {
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_callback_query().endpoint(handle_callback_query))
        .branch(
            Update::filter_chosen_inline_result()
                .branch(dptree::endpoint(handle_chosen_inline_result)),
        )
        .branch(Update::filter_inline_query().branch(dptree::endpoint(handle_inline_query)));

    let states = StateStorage::default();

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![db.clone(), states])
        .enable_ctrlc_handler()
        .build();

    dispatcher.dispatch().await;

    Ok(())
}

type StateStorage = Arc<Mutex<HashMap<UserId, State>>>;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Default)]
enum State {
    #[default]
    Start,
    MemeCreation {
        meme: memes::ActiveModel,
        translation: translations::ActiveModel,
        step: MemeCreationStep,
    },
    MemeEdition {
        meme_id: i32,
        language: String,
        action: MemeEditAction,
    },
}

#[derive(Clone)]
pub enum MemeCreationStep {
    Language,
    Title,
    Slug,
    Text,
    Source,
    Caption,
    Description,
}

fn make_keyboard(buttons: &[&str]) -> KeyboardMarkup {
    KeyboardMarkup::new([buttons.iter().map(|b| KeyboardButton::new(*b))]).resize_keyboard(true)
}

fn try_set_file_from_msg(msg: &Message, meme: &mut memes::ActiveModel) -> Result<Option<FileMeta>> {
    if let Some((file, thumb)) = if let Some([.., photo]) = msg.photo() {
        meme.media_type = ActiveValue::set(MediaType::Photo);
        meme.mime_type = ActiveValue::set(mime::IMAGE_JPEG.to_string());
        meme.width = ActiveValue::set(photo.width.try_into()?);
        meme.height = ActiveValue::set(photo.height.try_into()?);
        meme.duration = ActiveValue::set(0);
        Some((&photo.file, photo.clone()))
    } else if let Some(video) = msg.video() {
        meme.media_type = ActiveValue::set(MediaType::Video);
        meme.mime_type = ActiveValue::set(
            video
                .mime_type
                .clone()
                .context("no video mimetype")?
                .to_string(),
        );
        meme.width = ActiveValue::set(video.width.try_into()?);
        meme.height = ActiveValue::set(video.height.try_into()?);
        meme.duration = ActiveValue::set(video.duration.try_into()?);
        Some((&video.file, video.thumb.clone().context("no video thumb")?))
    } else if let Some(animation) = msg.animation() {
        meme.media_type = ActiveValue::set(MediaType::Animation);
        meme.mime_type = ActiveValue::set(
            animation
                .mime_type
                .clone()
                .context("no animation mimetype")?
                .to_string(),
        );
        meme.width = ActiveValue::set(animation.width.try_into()?);
        meme.height = ActiveValue::set(animation.height.try_into()?);
        meme.duration = ActiveValue::set(animation.duration.try_into()?);
        Some((
            &animation.file,
            animation.thumb.clone().context("no animation thumb")?,
        ))
    } else {
        None
    } {
        meme.tg_unique_id = ActiveValue::set(file.unique_id.clone());
        meme.tg_id = ActiveValue::set(file.id.clone());
        meme.content_length = ActiveValue::set(file.size.try_into()?);

        meme.thumb_mime_type = ActiveValue::set(mime::IMAGE_JPEG.to_string());
        meme.thumb_width = ActiveValue::set(thumb.width.try_into()?);
        meme.thumb_height = ActiveValue::set(thumb.height.try_into()?);
        meme.thumb_tg_id = ActiveValue::set(thumb.file.id);
        meme.thumb_content_length = ActiveValue::set(thumb.file.size.try_into()?);

        Ok(Some(file.clone()))
    } else {
        Ok(None)
    }
}

async fn handle_message(bot: Bot, msg: Message, db: Storage, states: StateStorage) -> Result<()> {
    let admin_chat_id: i64 = std::env::var("ADMIN_CHANNEL_ID")?.parse()?;
    let user = msg.from().context("no from")?.id;
    let state = states
        .lock()
        .unwrap()
        .get(&user)
        .cloned()
        .unwrap_or_default();

    if bot
        .get_chat_member(ChatId(admin_chat_id), msg.from().context("no from")?.id)
        .await?
        .is_present()
    {
        if let Some(text) = msg.text() {
            if text == "Отмена" {
                states.lock().unwrap().remove(&user);
                bot.send_message(msg.chat.id, "Отменено")
                    .reply_markup(KeyboardRemove::new())
                    .await?;
                return Ok(());
            } else if text == "/reindex" {
                db.reindex_all().await?;
                bot.send_message(msg.chat.id, "Reindex completed").await?;
                return Ok(());
            }
        }

        match state {
            State::Start => {
                let mut meme = memes::ActiveModel::new();

                if let Some(file) = try_set_file_from_msg(&msg, &mut meme)? {
                    if let Some(meme) = db.meme_by_tg_unique_id(&file.unique_id).await? {
                        bot.send_message(
                            msg.chat.id,
                            format!("https://t.me/c/{admin_chat_id}/{}", meme.control_message_id),
                        )
                        .await?;
                    } else {
                        meme.created_by = ActiveValue::set(msg.chat.id.0);
                        meme.last_edited_by = ActiveValue::set(msg.chat.id.0);
                        bot.send_message(msg.chat.id, "Теперь укажите язык для первого описания")
                            .reply_markup(make_keyboard(&["ru", "en"]))
                            .await?;
                        states.lock().unwrap().insert(
                            user,
                            State::MemeCreation {
                                meme,
                                translation: translations::ActiveModel::new(),
                                step: MemeCreationStep::Language,
                            },
                        );
                    }
                }
            }
            State::MemeCreation {
                mut meme,
                mut translation,
                step,
            } => {
                let text = msg.text().context("no text")?;

                if let Some(next_step) = match step {
                    MemeCreationStep::Language => {
                        translation.language = ActiveValue::set(text.to_owned());

                        bot.send_message(msg.chat.id, "Придумайте название")
                            .reply_markup(KeyboardRemove::new())
                            .await?;
                        Some(MemeCreationStep::Title)
                    }
                    MemeCreationStep::Title => {
                        translation.title = ActiveValue::set(text.to_owned());

                        let suggested_slug: String =
                            translit::Gost779B::new(translit::Language::Ru)
                                .to_latin(&text.to_lowercase().replace(' ', "-"))
                                .chars()
                                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                                .collect();

                        bot.send_message(
                            msg.chat.id,
                            format!("Теперь придумайте слаг, например `{suggested_slug}`"),
                        )
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(make_keyboard(&[&suggested_slug]))
                        .await?;

                        Some(MemeCreationStep::Slug)
                    }
                    MemeCreationStep::Slug => {
                        meme.slug = ActiveValue::set(text.to_owned());

                        bot.send_message(msg.chat.id, "Отправьте текст на меме")
                            .reply_markup(make_keyboard(&["Нет текста"]))
                            .await?;
                        Some(MemeCreationStep::Text)
                    }
                    MemeCreationStep::Text => {
                        if text != "Нет текста" {
                            meme.text = ActiveValue::set(Some(text.to_owned()));
                        };

                        bot.send_message(msg.chat.id, "Укажите url источника мема")
                            .reply_markup(make_keyboard(&["Неизвестен"]))
                            .await?;
                        Some(MemeCreationStep::Source)
                    }
                    MemeCreationStep::Source => {
                        if text != "Неизвестен" {
                            meme.source = ActiveValue::set(Some(text.to_owned()));
                        };

                        bot.send_message(msg.chat.id, "Придумайте подпись (короткое описание)")
                            .reply_markup(KeyboardRemove::new())
                            .await?;
                        Some(MemeCreationStep::Caption)
                    }
                    MemeCreationStep::Caption => {
                        translation.caption = ActiveValue::set(text.to_owned());

                        bot.send_message(msg.chat.id, "Придумайте описание")
                            .reply_markup(KeyboardRemove::new())
                            .await?;
                        Some(MemeCreationStep::Description)
                    }
                    MemeCreationStep::Description => {
                        translation.description = ActiveValue::set(text.to_owned());

                        let control_msg = db.create_meme(meme.clone(), translation.clone()).await?;

                        let control_msg_url = control_msg.url().context("can't create url")?;
                        bot.send_message(msg.chat.id, format!("Мем создан!\n{control_msg_url}"))
                            .reply_markup(KeyboardRemove::new())
                            .await?;
                        None
                    }
                } {
                    states.lock().unwrap().insert(
                        user,
                        State::MemeCreation {
                            meme,
                            translation,
                            step: next_step,
                        },
                    );
                } else {
                    states.lock().unwrap().remove(&user);
                };
            }
            State::MemeEdition {
                meme_id,
                language,
                action,
            } => {
                let updated_by = user.0.try_into()?;

                let mut meme = memes::ActiveModel {
                    id: ActiveValue::unchanged(meme_id),
                    ..Default::default()
                };
                let mut translation = translations::ActiveModel {
                    meme_id: ActiveValue::unchanged(meme_id),
                    language: ActiveValue::unchanged(language),
                    ..Default::default()
                };

                match action {
                    MemeEditAction::Slug => {
                        let text = msg.text().context("no text")?;
                        db.update_slug(meme_id, updated_by, text.to_owned()).await?;
                    }
                    MemeEditAction::Title => {
                        let text = msg.text().context("no text")?;
                        translation.title = ActiveValue::set(text.to_owned());
                        db.update_meme_translation(translation, updated_by).await?;
                    }
                    MemeEditAction::Caption => {
                        let text = msg.text().context("no text")?;
                        translation.caption = ActiveValue::set(text.to_owned());
                        db.update_meme_translation(translation, updated_by).await?;
                    }
                    MemeEditAction::Description => {
                        let text = msg.text().context("no text")?;
                        translation.description = ActiveValue::set(text.to_owned());
                        db.update_meme_translation(translation, updated_by).await?;
                    }
                    MemeEditAction::Text => {
                        let text = msg.text().context("no text")?;
                        meme.text = ActiveValue::set(if text != "Нет текста" {
                            Some(text.to_owned())
                        } else {
                            None
                        });
                        db.update_meme(meme, updated_by).await?;
                    }
                    MemeEditAction::Source => {
                        let text = msg.text().context("no text")?;
                        meme.source = ActiveValue::set(if text != "Неизвестен" {
                            Some(text.to_owned())
                        } else {
                            None
                        });
                        db.update_meme(meme, updated_by).await?;
                    }
                    MemeEditAction::File => {
                        if try_set_file_from_msg(&msg, &mut meme)?.is_some() {
                            db.update_meme(meme, updated_by).await?;
                        } else {
                            bot.send_message(msg.chat.id, "Нет файла или он не подходит")
                                .await?;
                            return Ok(());
                        }
                    }
                    MemeEditAction::Publish | MemeEditAction::Draft | MemeEditAction::Trash => {
                        unreachable!()
                    }
                };

                bot.send_message(msg.chat.id, "Мем обновлён!")
                    .reply_markup(KeyboardRemove::new())
                    .await?;
                states.lock().unwrap().remove(&user);
            }
        }
    } else {
        bot.send_message(msg.chat.id, "Добро пожаловать в поисковик мемов!\nЧтобы найти и отправить мем, \
        введите @memexpertbot и поисковый запрос в поле ввода сообщения в любом чате. Например, @memexpertbot вопрос огурец")
        .reply_markup(InlineKeyboardMarkup::new([[InlineKeyboardButton::switch_inline_query("Искать мемы", "")]])).await?;
    }
    Ok(())
}

async fn handle_inline_query(bot: Bot, query: InlineQuery, db: Storage) -> Result<()> {
    let memes = db
        .search_memes(query.from.id.0.try_into()?, &query.query)
        .await?
        .into_iter()
        .map(|meme| {
            let id = format!("{}:{}:{}", meme.2, meme.1, meme.0.id);
            match meme.0.media_type {
                MediaType::Photo => InlineQueryResult::CachedPhoto(
                    InlineQueryResultCachedPhoto::new(id, meme.0.tg_id),
                ),
                MediaType::Video => InlineQueryResult::CachedVideo(
                    InlineQueryResultCachedVideo::new(id, meme.0.tg_id, meme.0.slug),
                ),
                MediaType::Animation => {
                    InlineQueryResult::CachedGif(InlineQueryResultCachedGif::new(id, meme.0.tg_id))
                }
            }
        });
    bot.answer_inline_query(query.id, memes)
        .cache_time(0)
        .await?;
    Ok(())
}

async fn handle_chosen_inline_result(chosen: ChosenInlineResult, db: Storage) -> Result<()> {
    let splitten = chosen.result_id.split(':').collect_vec();
    let [use_id, meme_source, meme_id] = splitten[..] else {
        bail!("invalid id")
    };
    db.save_tg_chosen(
        use_id.parse()?,
        chosen.from.id.0.try_into()?,
        meme_id.parse()?,
        meme_source.chars().next().context("empty source")?,
    )
    .await?;
    Ok(())
}

async fn handle_callback_query(
    bot: Bot,
    q: CallbackQuery,
    db: Storage,
    states: StateStorage,
) -> Result<()> {
    let data = q.data.context("no data")?;
    let callback: MemeEditCallback = data.parse()?;
    let user_id = q.from.id;

    let mut meme = memes::ActiveModel {
        id: ActiveValue::unchanged(callback.meme_id),
        ..Default::default()
    };

    match callback.action {
        MemeEditAction::Slug => {
            bot.send_message(user_id, "Отправьте новый слаг").await?;
        }
        MemeEditAction::Title => {
            bot.send_message(
                user_id,
                format!("Отправьте новый заголовок ({})", callback.language),
            )
            .await?;
        }
        MemeEditAction::Description => {
            bot.send_message(
                user_id,
                format!("Отправьте новое описание ({})", callback.language),
            )
            .await?;
        }
        MemeEditAction::Caption => {
            bot.send_message(
                user_id,
                format!("Отправьте новую подпись ({})", callback.language),
            )
            .await?;
        }
        MemeEditAction::Text => {
            bot.send_message(user_id, "Отправьте новый текст").await?;
        }
        MemeEditAction::Source => {
            bot.send_message(user_id, "Отправьте новый источник")
                .reply_markup(make_keyboard(&["Неизвестен"]))
                .await?;
        }
        MemeEditAction::Publish => {
            meme.publish_status = ActiveValue::set(PublishStatus::Published);
            db.update_meme(meme, user_id.0.try_into()?).await?;
            bot.answer_callback_query(q.id).await?;
            return Ok(());
        }
        MemeEditAction::Draft => {
            meme.publish_status = ActiveValue::set(PublishStatus::Draft);
            db.update_meme(meme, user_id.0.try_into()?).await?;
            bot.answer_callback_query(q.id).await?;
            return Ok(());
        }
        MemeEditAction::Trash => {
            meme.publish_status = ActiveValue::set(PublishStatus::Trash);
            db.update_meme(meme, user_id.0.try_into()?).await?;
            bot.answer_callback_query(q.id).await?;
            return Ok(());
        }
        MemeEditAction::File => {
            bot.send_message(user_id, "Отправьте новый файл").await?;
        }
    }
    states.lock().unwrap().insert(
        q.from.id,
        State::MemeEdition {
            meme_id: callback.meme_id,
            language: callback.language,
            action: callback.action,
        },
    );
    bot.answer_callback_query(q.id).await?;

    Ok(())
}
