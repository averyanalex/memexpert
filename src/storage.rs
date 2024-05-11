use std::fmt::Write;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use entities::{
    files_cache, memes,
    prelude::*,
    sea_orm_active_enums::{MediaType, PublishStatus},
    slug_redirects, tg_uses, translations, web_visits,
};
use itertools::Itertools;
use meilisearch_sdk::client::Client;
use migration::{Migrator, MigratorTrait, OnConflict};
use qdrant_client::{
    client::{Payload, QdrantClient},
    qdrant::{
        point_id::PointIdOptions, points_selector::PointsSelectorOneOf, vectors_config::Config,
        CreateCollection, Distance, PointStruct, PointsIdsList, PointsSelector, SearchPoints,
        VectorParams, VectorsConfig,
    },
};
use sea_orm::{
    prelude::*, ActiveValue, ConnectOptions, Database, IntoActiveModel, QueryOrder, QuerySelect,
    TransactionTrait,
};
use teloxide::{net::Download, requests::Requester, types::Message, Bot};
use tracing::log::LevelFilter;

use crate::{
    control::refresh_meme_control_msg,
    ms_models::{MsMeme, MsMemeResult, MsMemeTranslation},
    yandex::Yandex,
};

#[derive(Clone)]
pub struct Storage {
    dc: DatabaseConnection,
    ms: Client,
    qd: Arc<QdrantClient>,
    bot: Bot,
    yandex: Arc<Yandex>,
}

impl Storage {
    pub async fn new(bot: Bot, yandex: Arc<Yandex>) -> Result<Self> {
        let db_url = std::env::var("DATABASE_URL")?;

        let mut conn_options = ConnectOptions::new(db_url);
        conn_options.sqlx_logging_level(LevelFilter::Debug);
        conn_options.sqlx_logging(true);

        let dc = Database::connect(conn_options).await?;
        Migrator::up(&dc, None).await?;

        let ms = Client::new("http://127.0.0.1:7700", None::<String>)?;

        let qd = Arc::new(QdrantClient::from_url("http://127.0.0.1:6334").build()?);
        if !qd.collection_exists("memexpert-text").await? {
            qd.create_collection(&CreateCollection {
                collection_name: "memexpert-text".to_owned(),
                vectors_config: Some(VectorsConfig {
                    config: Some(Config::Params(VectorParams {
                        size: 256,
                        distance: Distance::Cosine.into(),
                        ..Default::default()
                    })),
                }),
                ..Default::default()
            })
            .await?;
        }

        Ok(Self {
            dc,
            ms,
            qd,
            bot,
            yandex,
        })
    }

    async fn create_or_replace_meme_in_ms(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<()> {
        if meme.publish_status == PublishStatus::Published {
            let meme = MsMeme {
                id: meme.id,
                text: meme.text.clone(),
                translations: translations
                    .iter()
                    .map(|t| {
                        (
                            t.language.clone(),
                            MsMemeTranslation {
                                title: t.title.clone(),
                                caption: t.caption.clone(),
                                description: t.description.clone(),
                            },
                        )
                    })
                    .collect(),
            };
            self.ms
                .index("memexpert")
                .add_or_replace(&[meme], Some("id"))
                .await?;
        } else {
            self.ms.index("memexpert").delete_document(meme.id).await?;
        }

        Ok(())
    }

    async fn create_or_replace_meme_in_qd(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<()> {
        if meme.publish_status == PublishStatus::Published {
            let text_embedding = self.get_text_embedding(meme, translations).await?;
            self.qd
                .upsert_points_blocking(
                    "memexpert-text",
                    None,
                    vec![PointStruct::new(
                        u64::try_from(meme.id)?,
                        text_embedding,
                        Payload::new(),
                    )],
                    None,
                )
                .await?;
        } else {
            self.qd
                .delete_points_blocking(
                    "memexpert-text",
                    None,
                    &PointsSelector {
                        points_selector_one_of: Some(PointsSelectorOneOf::Points(PointsIdsList {
                            ids: vec![u64::try_from(meme.id)?.into()],
                        })),
                    },
                    None,
                )
                .await?;
        }

        Ok(())
    }

    async fn get_text_embedding(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<Vec<f32>> {
        fn add_dot_if_needed(text: &str) -> String {
            let last_char = text.chars().last().unwrap_or('.');
            if last_char == '.' || last_char == '!' || last_char == '?' {
                text.to_owned()
            } else {
                format!("{text}.")
            }
        }

        let meme_type = match meme.media_type {
            MediaType::Animation => "gif",
            MediaType::Photo => "image",
            MediaType::Video => "video",
        };
        let text_on_meme = if let Some(text) = &meme.text {
            format!("Text on meme: {}", add_dot_if_needed(text))
        } else {
            "There is no text on the meme.".to_string()
        };

        let mut text = format!(
            "Meme id: {}.\nMeme type: {meme_type}.\n{text_on_meme}",
            meme.slug
        );

        for translation in translations {
            write!(
                text,
                "\n\nTranslation for {} language:\nTitle: {}\nCaption: {}\nDescription: {}",
                translation.language,
                add_dot_if_needed(&translation.title),
                add_dot_if_needed(&translation.caption),
                add_dot_if_needed(&translation.description)
            )?;
        }

        self.yandex.text_embedding(text, "text-search-doc").await
    }

    async fn process_meme_update(
        &self,
        trans: &impl ConnectionTrait,
        meme_id: i32,
    ) -> Result<Option<Message>> {
        let (meme, translations) = Memes::find()
            .find_with_related(Translations)
            .filter(memes::Column::Id.eq(meme_id))
            .all(trans)
            .await?
            .into_iter()
            .next()
            .context("meme not found")?;

        let control_msg = refresh_meme_control_msg(&self.bot, &meme, &translations).await?;

        if let Some(control_msg) = &control_msg {
            memes::ActiveModel {
                id: ActiveValue::unchanged(meme.id),
                control_message_id: ActiveValue::set(control_msg.id.0),
                ..Default::default()
            }
            .save(trans)
            .await?;
        }

        self.create_or_replace_meme_in_ms(&meme, &translations)
            .await?;
        self.create_or_replace_meme_in_qd(&meme, &translations)
            .await?;

        self.load_tg_file(&meme.tg_id, meme.content_length.try_into()?)
            .await?;
        self.load_tg_file(&meme.thumb_tg_id, meme.thumb_content_length.try_into()?)
            .await?;

        Ok(control_msg)
    }

    pub async fn create_meme(
        &self,
        mut meme: memes::ActiveModel,
        mut translation: translations::ActiveModel,
    ) -> Result<Message> {
        let trans = self.dc.begin().await?;

        meme.control_message_id = ActiveValue::set(-1);
        let meme = Memes::insert(meme)
            .exec_with_returning(&trans)
            .await
            .unwrap();

        translation.meme_id = ActiveValue::set(meme.id);
        Translations::insert(translation).exec(&trans).await?;

        let control_msg = self
            .process_meme_update(&trans, meme.id)
            .await?
            .context("must create control message")?;

        trans.commit().await?;
        Ok(control_msg)
    }

    pub async fn meme_with_translations_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<(memes::Model, Vec<translations::Model>)>> {
        Ok(Memes::find()
            .find_with_related(Translations)
            .filter(memes::Column::Slug.eq(slug))
            .all(&self.dc)
            .await?
            .into_iter()
            .next())
    }

    pub async fn meme_by_tg_unique_id(&self, tg_unique_id: &str) -> Result<Option<memes::Model>> {
        Ok(Memes::find()
            .filter(memes::Column::TgUniqueId.eq(tg_unique_id))
            .one(&self.dc)
            .await?)
    }

    pub async fn update_slug(&self, meme_id: i32, updated_by: i64, slug: String) -> Result<()> {
        let trans = self.dc.begin().await?;

        let meme = Memes::find_by_id(meme_id)
            .one(&trans)
            .await?
            .context("meme not found")?;

        SlugRedirects::insert(slug_redirects::ActiveModel {
            slug: ActiveValue::set(meme.slug.clone()),
            meme_id: ActiveValue::set(meme_id),
        })
        .on_conflict(
            OnConflict::column(slug_redirects::Column::Slug)
                .update_column(slug_redirects::Column::MemeId)
                .to_owned(),
        )
        .exec(&trans)
        .await?;

        let mut meme = meme.into_active_model();
        meme.slug = ActiveValue::set(slug);

        self.update_meme_internal(&trans, meme, updated_by).await?;

        trans.commit().await?;
        Ok(())
    }

    async fn update_meme_internal(
        &self,
        trans: &impl ConnectionTrait,
        mut meme: memes::ActiveModel,
        updated_by: i64,
    ) -> Result<()> {
        let meme_id = meme.id.clone().unwrap();

        meme.last_edited_by = ActiveValue::set(updated_by);
        meme.last_edition_time = ActiveValue::set(Utc::now().naive_utc());
        meme.save(trans).await?;

        self.process_meme_update(trans, meme_id).await?;

        Ok(())
    }

    pub async fn update_meme(&self, meme: memes::ActiveModel, updated_by: i64) -> Result<()> {
        let trans = self.dc.begin().await?;
        self.update_meme_internal(&trans, meme, updated_by).await?;
        trans.commit().await?;
        Ok(())
    }

    pub async fn update_meme_translation(
        &self,
        translation: translations::ActiveModel,
        updated_by: i64,
    ) -> Result<()> {
        let trans = self.dc.begin().await?;
        let meme_id = translation.meme_id.clone().unwrap();

        translation.save(&trans).await?;

        self.update_meme_internal(
            &trans,
            memes::ActiveModel {
                id: ActiveValue::unchanged(meme_id),
                ..Default::default()
            },
            updated_by,
        )
        .await?;

        trans.commit().await?;
        Ok(())
    }

    pub async fn search_memes(
        &self,
        user_id: i64,
        query: &str,
    ) -> Result<Vec<(memes::Model, char, i64)>> {
        let tg_use = TgUses::insert(tg_uses::ActiveModel {
            user_id: ActiveValue::set(user_id),
            query: ActiveValue::set(if query.is_empty() {
                None
            } else {
                Some(query.to_owned())
            }),
            ..Default::default()
        })
        .exec_with_returning(&self.dc)
        .await?;

        let ids = if query.is_empty() {
            TgUses::find()
                .filter(tg_uses::Column::UserId.eq(user_id))
                .filter(tg_uses::Column::ChosenMemeId.is_not_null())
                .order_by_desc(tg_uses::Column::Id)
                .limit(1024)
                .all(&self.dc)
                .await?
                .into_iter()
                .map(|m| m.chosen_meme_id.unwrap_or(-1))
                .unique()
                .map(|i| (i, 'r'))
                .collect_vec()
        } else {
            let (qd_results, ms_results): (Result<_>, _) = tokio::join!(
                async {
                    let query_embedding = self
                        .yandex
                        .text_embedding(query, "text-search-query")
                        .await?;
                    Ok(self
                        .qd
                        .search_points(&SearchPoints {
                            collection_name: "memexpert-text".to_owned(),
                            vector: query_embedding,
                            limit: 100,
                            ..Default::default()
                        })
                        .await?
                        .result)
                },
                async {
                    self.ms
                        .index("memexpert")
                        .search()
                        .with_query(query)
                        .with_limit(100)
                        .with_show_ranking_score(true)
                        .execute::<MsMemeResult>()
                        .await
                }
            );

            let qd_ids_scores = qd_results?
                .into_iter()
                .map(|r| {
                    (
                        match r.id.unwrap_or_default().point_id_options.unwrap() {
                            PointIdOptions::Num(n) => n as i32,
                            PointIdOptions::Uuid(_) => -1,
                        },
                        r.score,
                        't',
                    )
                })
                .collect_vec();
            let mut ms_ids_scores = ms_results?
                .hits
                .into_iter()
                .map(|r| (r.result.id, r.ranking_score.unwrap_or(0.0) as f32, 'm'))
                .collect_vec();

            let ms_coef = qd_ids_scores.iter().map(|r| r.1).next().unwrap_or(0.5)
                / ms_ids_scores.iter().map(|r| r.1).next().unwrap_or(1.0);
            for ids in &mut ms_ids_scores {
                ids.1 = ids.1 * ms_coef - 0.01;
            }

            let mut ids_scores = qd_ids_scores;
            ids_scores.extend(ms_ids_scores);
            ids_scores.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            ids_scores
                .into_iter()
                .map(|i| (i.0, i.2))
                .unique_by(|i| i.0)
                .collect_vec()
        };

        let memes = Memes::find()
            .filter(memes::Column::Id.is_in(ids.iter().map(|i| i.0)))
            .filter(memes::Column::PublishStatus.eq(PublishStatus::Published))
            .order_by_asc(memes::Column::Id)
            .all(&self.dc)
            .await?;
        let memes = ids
            .into_iter()
            .filter_map(|i| {
                if let Ok(idx) = memes.binary_search_by_key(&i.0, |m| m.id) {
                    Some((memes[idx].clone(), i.1, tg_use.id))
                } else {
                    None
                }
            })
            .take(50)
            .collect();
        Ok(memes)
    }

    pub async fn all_memes_with_translations(
        &self,
    ) -> Result<Vec<(memes::Model, Vec<translations::Model>)>> {
        let memes = Memes::find()
            .find_with_related(Translations)
            .all(&self.dc)
            .await?;
        Ok(memes)
    }

    pub async fn save_tg_chosen(
        &self,
        id: i64,
        user_id: i64,
        meme_id: i32,
        meme_source: char,
    ) -> Result<()> {
        tg_uses::ActiveModel {
            id: ActiveValue::unchanged(id),
            user_id: ActiveValue::unchanged(user_id),
            chosen_meme_id: ActiveValue::set(Some(meme_id)),
            chosen_meme_source: ActiveValue::set(Some(meme_source.to_string())),
            ..Default::default()
        }
        .save(&self.dc)
        .await?;
        Ok(())
    }

    pub async fn create_web_visit(&self, visit: web_visits::ActiveModel) -> Result<()> {
        WebVisits::insert(visit).exec(&self.dc).await?;
        Ok(())
    }

    pub async fn load_tg_file(&self, id: &str, size: usize) -> Result<Vec<u8>> {
        if let Some(cached) = FilesCache::find_by_id(id).one(&self.dc).await? {
            Ok(cached.data)
        } else {
            let mut dst = Vec::with_capacity(size);
            let file = self.bot.get_file(id).await?;
            self.bot.download_file(&file.path, &mut dst).await?;
            files_cache::ActiveModel {
                id: ActiveValue::set(id.to_owned()),
                data: ActiveValue::set(dst.clone()),
            }
            .insert(&self.dc)
            .await?;
            Ok(dst)
        }
    }
}
