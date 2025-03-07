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
    adaptors::throttle::Limits,
    prelude::*,
    types::{
        ChatAction, FileMeta, InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult,
        InlineQueryResultCachedGif, InlineQueryResultCachedPhoto, InlineQueryResultCachedVideo,
        KeyboardButton, KeyboardMarkup, KeyboardRemove, MessageId, PhotoSize, ReplyParameters,
    },
};
use tracing::*;

use crate::{
    ai::{AiMetadata, JinaTaskType},
    control::{MemeEditAction, MemeEditCallback},
    AppState,
};

pub type Bot = teloxide::adaptors::Throttle<teloxide::adaptors::CacheMe<teloxide::Bot>>;

pub fn new_bot() -> Bot {
    teloxide::Bot::from_env()
        .cache_me()
        .throttle(Limits::default())
}

pub async fn run_bot(app_state: AppState) -> Result<()> {
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_callback_query().endpoint(handle_callback_query))
        .branch(
            Update::filter_chosen_inline_result()
                .branch(dptree::endpoint(handle_chosen_inline_result)),
        )
        .branch(Update::filter_inline_query().branch(dptree::endpoint(handle_inline_query)));

    let bot_state = BotState::default();

    let mut dispatcher = Dispatcher::builder(app_state.bot.clone(), handler)
        .dependencies(dptree::deps![app_state.clone(), bot_state])
        .enable_ctrlc_handler()
        .build();

    let me = app_state.bot.get_me().await?;
    info!("running bot as @{}", me.username());

    dispatcher.dispatch().await;

    Ok(())
}

#[derive(Clone, Default)]
enum ChatState {
    #[default]
    Start,
    MemeEdition {
        meme_id: i32,
        language: String,
        action: MemeEditAction,
    },
}
struct UserSettings {
    cheap_model: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self { cheap_model: true }
    }
}

#[derive(Default)]
struct BotState_ {
    chat_states: Mutex<HashMap<UserId, ChatState>>,
    user_tmp_settings: Mutex<HashMap<UserId, UserSettings>>,
    meme_creation_confirmations: Mutex<HashMap<(UserId, MessageId), MemeCreationData>>,
}

type BotState = Arc<BotState_>;

struct MemeCreationData {
    msg: Message,
    thumb_file_id: String,
    thumb_file_size: usize,
    meme: memes::ActiveModel,
    img_embedding: Vec<f32>,
}

fn make_keyboard(buttons: &[&str]) -> KeyboardMarkup {
    KeyboardMarkup::new([buttons.iter().map(|b| KeyboardButton::new(*b))]).resize_keyboard()
}

fn try_set_file_from_msg(
    msg: &Message,
    meme: &mut memes::ActiveModel,
) -> Result<Option<(FileMeta, PhotoSize)>> {
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
        meme.duration = ActiveValue::set(video.duration.seconds().try_into()?);
        Some((
            &video.file,
            video.thumbnail.clone().context("no video thumb")?,
        ))
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
        meme.duration = ActiveValue::set(animation.duration.seconds().try_into()?);
        Some((
            &animation.file,
            animation.thumbnail.clone().context("no animation thumb")?,
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
        meme.thumb_tg_id = ActiveValue::set(thumb.file.id.clone());
        meme.thumb_content_length = ActiveValue::set(thumb.file.size.try_into()?);

        Ok(Some((file.clone(), thumb)))
    } else {
        Ok(None)
    }
}

fn get_admin_chat_id() -> Result<i64> {
    Ok(std::env::var("ADMIN_CHANNEL_ID")?.parse()?)
}

async fn is_user_admin(app_state: &AppState, user: UserId) -> Result<bool> {
    Ok(app_state
        .bot
        .get_chat_member(ChatId(get_admin_chat_id()?), user)
        .await?
        .is_present())
}

async fn finish_meme_creation(
    app_state: &AppState,
    bot_state: &BotState,
    mut data: MemeCreationData,
) -> Result<()> {
    data.meme.created_by = ActiveValue::set(data.msg.chat.id.0);
    data.meme.last_edited_by = ActiveValue::set(data.msg.chat.id.0);

    let is_cheap_model = bot_state
        .user_tmp_settings
        .lock()
        .unwrap()
        .entry(data.msg.from.context("no from")?.id)
        .or_default()
        .cheap_model;
    let ai_meta = app_state
        .ai
        .gen_new_meme_metadata(
            app_state
                .storage
                .load_tg_file(&data.thumb_file_id, data.thumb_file_size)
                .await?,
            is_cheap_model,
        )
        .await?;

    let mut translation = translations::ActiveModel::new();
    translation.language = ActiveValue::set("ru".to_owned());

    ai_meta.apply(&mut data.meme, &mut translation);
    data.meme.publish_status = ActiveValue::set(PublishStatus::Published);

    let control_msg = app_state
        .storage
        .create_meme(data.meme, translation, data.img_embedding)
        .await?;
    let control_msg_url = control_msg.url().context("can't create url")?;

    app_state
        .bot
        .send_message(data.msg.chat.id, format!("Мем создан!\n{control_msg_url}"))
        .reply_markup(KeyboardRemove::new())
        .await?;

    Ok(())
}

async fn process_meme_creation(
    app_state: &AppState,
    bot_state: &BotState,
    msg: &Message,
) -> Result<()> {
    let mut meme = memes::ActiveModel::new();
    let admin_chat_id = get_admin_chat_id()?;

    if let Some((file, thumb)) = try_set_file_from_msg(msg, &mut meme)? {
        if let Some(meme) = app_state
            .storage
            .load_meme_by_tg_unique_id(&file.unique_id)
            .await?
        {
            app_state
                .bot
                .send_message(
                    msg.chat.id,
                    format!(
                        "Мем уже существует: https://t.me/c/{}/{}",
                        -admin_chat_id % 10_000_000_000,
                        meme.control_message_id
                    ),
                )
                .await?;
        } else {
            app_state
                .bot
                .send_chat_action(msg.chat.id, ChatAction::Typing)
                .await?;
            let thumb_data = app_state
                .storage
                .load_tg_file(&thumb.file.id, thumb.file.size as usize)
                .await?;
            let embedding = app_state
                .ai
                .jina_clip(thumb_data.into(), JinaTaskType::Passage)
                .await?;

            let meme_creation_data = MemeCreationData {
                msg: msg.clone(),
                thumb_file_id: thumb.file.id.clone(),
                thumb_file_size: thumb.file.size as usize,
                meme,
                img_embedding: embedding.clone(),
            };

            if let Some(found_meme) = app_state.storage.find_similar_image(embedding).await? {
                let sent_msg = app_state
                    .bot
                    .send_message(
                        msg.chat.id,
                        format!(
                            "Очень похожий мем: https://t.me/c/{}/{}, продолжить?",
                            -admin_chat_id % 10_000_000_000,
                            found_meme.control_message_id
                        ),
                    )
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .reply_markup(InlineKeyboardMarkup::new([vec![
                        InlineKeyboardButton::callback("Создать", "confirm"),
                    ]]))
                    .await?;
                bot_state
                    .meme_creation_confirmations
                    .lock()
                    .unwrap()
                    .insert(
                        (msg.from.clone().context("no user")?.id, sent_msg.id),
                        meme_creation_data,
                    );
            } else {
                finish_meme_creation(app_state, bot_state, meme_creation_data).await?;
            }
        }
    }

    Ok(())
}

async fn process_meme_edition(
    app_state: &AppState,
    bot_state: &BotState,
    user: UserId,
    msg: &Message,
    meme_id: i32,
    language: String,
    action: MemeEditAction,
) -> Result<()> {
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
        MemeEditAction::Ai => {
            let prompt = msg.text().context("no text")?;
            let (current_meme_ver, translations) = app_state
                .storage
                .load_meme_with_translations_by_id(meme_id)
                .await?
                .context("meme not found")?;
            let ru_translation = translations
                .into_iter()
                .find(|t| t.language == "ru")
                .context("no ru translation")?;

            let thumb = app_state
                .storage
                .load_tg_file(
                    &current_meme_ver.thumb_tg_id,
                    current_meme_ver.thumb_content_length.try_into()?,
                )
                .await?;
            let new_metadata = app_state
                .ai
                .generate_edited_meme_metadata(
                    AiMetadata::from_meme_with_translation(current_meme_ver, ru_translation),
                    thumb,
                    prompt,
                )
                .await?;

            new_metadata.apply(&mut meme, &mut translation);
            translation.language = ActiveValue::unchanged("ru".to_owned());

            app_state
                .storage
                .update_meme(meme, vec![translation], updated_by)
                .await?;
        }
        MemeEditAction::Slug => {
            let text = msg.text().context("no text")?;
            meme.slug = ActiveValue::set(text.to_owned());
            app_state
                .storage
                .update_meme(meme, vec![], updated_by)
                .await?;
        }
        MemeEditAction::Title => {
            let text = msg.text().context("no text")?;
            translation.title = ActiveValue::set(text.to_owned());
            app_state
                .storage
                .update_meme(meme, vec![translation], updated_by)
                .await?;
        }
        MemeEditAction::Caption => {
            let text = msg.text().context("no text")?;
            translation.caption = ActiveValue::set(text.to_owned());
            app_state
                .storage
                .update_meme(meme, vec![translation], updated_by)
                .await?;
        }
        MemeEditAction::Description => {
            let text = msg.text().context("no text")?;
            translation.description = ActiveValue::set(text.to_owned());
            app_state
                .storage
                .update_meme(meme, vec![translation], updated_by)
                .await?;
        }
        MemeEditAction::Text => {
            let text = msg.text().context("no text")?;
            meme.text = ActiveValue::set(if text != "Нет текста" {
                Some(text.to_owned())
            } else {
                None
            });
            app_state
                .storage
                .update_meme(meme, vec![], updated_by)
                .await?;
        }
        MemeEditAction::Source => {
            let text = msg.text().context("no text")?;
            meme.source = ActiveValue::set(if text != "Неизвестен" {
                Some(text.to_owned())
            } else {
                None
            });
            app_state
                .storage
                .update_meme(meme, vec![], updated_by)
                .await?;
        }
        MemeEditAction::File => {
            if try_set_file_from_msg(msg, &mut meme)?.is_some() {
                app_state
                    .storage
                    .update_meme(meme, vec![], updated_by)
                    .await?;
            } else {
                app_state
                    .bot
                    .send_message(msg.chat.id, "Нет файла или он не подходит")
                    .await?;
                return Ok(());
            }
        }
        MemeEditAction::Publish | MemeEditAction::Draft | MemeEditAction::Trash => {
            unreachable!()
        }
    };

    app_state
        .bot
        .send_message(msg.chat.id, "Мем обновлён!")
        .reply_markup(KeyboardRemove::new())
        .await?;
    bot_state.chat_states.lock().unwrap().remove(&user);

    Ok(())
}

async fn handle_message(app_state: AppState, bot_state: BotState, msg: Message) -> Result<()> {
    let user = msg.from.clone().context("no from")?.id;

    if is_user_admin(&app_state, user).await? {
        let state = bot_state
            .chat_states
            .lock()
            .unwrap()
            .get(&user)
            .cloned()
            .unwrap_or_default();

        if let Some(text) = msg.text() {
            if text == "Отмена" {
                bot_state.chat_states.lock().unwrap().remove(&user);
                app_state
                    .bot
                    .send_message(msg.chat.id, "Отменено")
                    .reply_markup(KeyboardRemove::new())
                    .await?;
                return Ok(());
            } else if text == "/reindex" {
                app_state.storage.reindex_all().await?;
                app_state
                    .bot
                    .send_message(msg.chat.id, "Reindex completed")
                    .await?;
                return Ok(());
            } else if text == "/heal" {
                app_state.storage.heal_qd().await?;
                app_state
                    .bot
                    .send_message(msg.chat.id, "Heal completed")
                    .await?;
                return Ok(());
            } else if text == "/retgmsg" {
                app_state.storage.refresh_all_control_messages().await?;
                app_state
                    .bot
                    .send_message(msg.chat.id, "Control messages refresh completed")
                    .await?;
                return Ok(());
            } else if text == "/smart" || text == "/dumb" {
                bot_state
                    .user_tmp_settings
                    .lock()
                    .unwrap()
                    .entry(user)
                    .or_default()
                    .cheap_model = text == "/dumb";
                app_state
                    .bot
                    .send_message(msg.chat.id, "Model changed")
                    .await?;
                return Ok(());
            }
        }

        match state {
            ChatState::Start => process_meme_creation(&app_state, &bot_state, &msg).await?,
            ChatState::MemeEdition {
                meme_id,
                language,
                action,
            } => {
                process_meme_edition(
                    &app_state, &bot_state, user, &msg, meme_id, language, action,
                )
                .await?
            }
        }
    } else {
        app_state.bot.send_message(msg.chat.id, "Добро пожаловать в поисковик мемов!\nЧтобы найти и отправить мем, \
        введите @memexpertbot и поисковый запрос в поле ввода сообщения в любом чате. Например, @memexpertbot вопрос огурец")
        .reply_markup(InlineKeyboardMarkup::new([[InlineKeyboardButton::switch_inline_query("Искать мемы", "")]])).await?;
    }
    Ok(())
}

async fn handle_inline_query(app_state: AppState, query: InlineQuery) -> Result<()> {
    let tg_use = app_state
        .storage
        .create_tg_use(query.from.id, &query.query)
        .await?;

    let meme_models: Vec<_> = if query.query.is_empty() {
        let recent = app_state.storage.recent_memes(query.from.id, 30).await?;
        let popular = app_state.storage.popular_memes(50).await?;
        recent
            .into_iter()
            .map(|m| (m, 'r'))
            .chain(popular.into_iter().map(|m| (m, 'p')))
            .collect()
    } else {
        app_state
            .storage
            .search_memes(&query.query, Default::default())
            .await?
            .into_iter()
            .map(|m| (m, 'q'))
            .collect()
    };

    let memes = meme_models
        .into_iter()
        .unique_by(|m| m.0.id)
        .take(50)
        .map(|meme| {
            let id = format!("{}:{}:{}", tg_use.id, meme.1, meme.0.id);
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

    app_state
        .bot
        .answer_inline_query(query.id, memes)
        .cache_time(0)
        .await?;
    Ok(())
}

async fn handle_chosen_inline_result(
    app_state: AppState,
    chosen: ChosenInlineResult,
) -> Result<()> {
    let splitten = chosen.result_id.split(':').collect_vec();
    let [use_id, meme_source, meme_id] = splitten[..] else {
        bail!("invalid id")
    };
    app_state
        .storage
        .save_tg_chosen(
            use_id.parse()?,
            chosen.from.id.0.try_into()?,
            meme_id.parse()?,
            meme_source.chars().next().context("empty source")?,
        )
        .await?;
    Ok(())
}

async fn handle_callback_query(
    app_state: AppState,
    bot_state: BotState,
    q: CallbackQuery,
) -> Result<()> {
    let data = q.data.context("no data")?;

    if data == "confirm" {
        let data = bot_state
            .meme_creation_confirmations
            .lock()
            .unwrap()
            .remove(&(q.from.id, q.message.context("no message")?.id()));
        if let Some(data) = data {
            finish_meme_creation(&app_state, &bot_state, data).await?;
        };
    } else {
        let callback: MemeEditCallback = data.parse()?;
        let user_id = q.from.id;

        let mut meme = memes::ActiveModel {
            id: ActiveValue::unchanged(callback.meme_id),
            ..Default::default()
        };

        match callback.action {
            MemeEditAction::Ai => {
                app_state
                    .bot
                    .send_message(user_id, "Отправьте промпт для редактирования")
                    .await?;
            }
            MemeEditAction::Slug => {
                app_state
                    .bot
                    .send_message(user_id, "Отправьте новый слаг")
                    .await?;
            }
            MemeEditAction::Title => {
                app_state
                    .bot
                    .send_message(
                        user_id,
                        format!("Отправьте новый заголовок ({})", callback.language),
                    )
                    .await?;
            }
            MemeEditAction::Description => {
                app_state
                    .bot
                    .send_message(
                        user_id,
                        format!("Отправьте новое описание ({})", callback.language),
                    )
                    .await?;
            }
            MemeEditAction::Caption => {
                app_state
                    .bot
                    .send_message(
                        user_id,
                        format!("Отправьте новую подпись ({})", callback.language),
                    )
                    .await?;
            }
            MemeEditAction::Text => {
                app_state
                    .bot
                    .send_message(user_id, "Отправьте новый текст")
                    .await?;
            }
            MemeEditAction::Source => {
                app_state
                    .bot
                    .send_message(user_id, "Отправьте новый источник")
                    .reply_markup(make_keyboard(&["Неизвестен"]))
                    .await?;
            }
            MemeEditAction::Publish => {
                meme.publish_status = ActiveValue::set(PublishStatus::Published);
                app_state
                    .storage
                    .update_meme(meme, vec![], user_id.0.try_into()?)
                    .await?;
                app_state.bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
            MemeEditAction::Draft => {
                meme.publish_status = ActiveValue::set(PublishStatus::Draft);
                app_state
                    .storage
                    .update_meme(meme, vec![], user_id.0.try_into()?)
                    .await?;
                app_state.bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
            MemeEditAction::Trash => {
                meme.publish_status = ActiveValue::set(PublishStatus::Trash);
                app_state
                    .storage
                    .update_meme(meme, vec![], user_id.0.try_into()?)
                    .await?;
                app_state.bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
            MemeEditAction::File => {
                app_state
                    .bot
                    .send_message(user_id, "Отправьте новый файл")
                    .await?;
            }
        }
        bot_state.chat_states.lock().unwrap().insert(
            q.from.id,
            ChatState::MemeEdition {
                meme_id: callback.meme_id,
                language: callback.language,
                action: callback.action,
            },
        );
    }

    app_state.bot.answer_callback_query(q.id).await?;

    Ok(())
}
