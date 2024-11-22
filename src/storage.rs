use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use itertools::Itertools;
use tokio::time;
use tracing::log::LevelFilter;

use entities::{
    files_cache, memes, prelude::*, sea_orm_active_enums::PublishStatus, slug_redirects, tg_uses,
    translations, web_visits,
};
use migration::{Migrator, MigratorTrait};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    prelude::*, ActiveValue, ConnectOptions, Database, DatabaseTransaction, FromQueryResult,
    IntoActiveModel, Order, QueryOrder, QuerySelect, TransactionTrait,
};

use qdrant_client::qdrant::{Fusion, PrefetchQueryBuilder, Query as QdQuery, QueryPointsBuilder};
use qdrant_client::{
    client::Payload,
    qdrant::{
        point_id::PointIdOptions, CreateCollectionBuilder, DeletePointsBuilder, Distance,
        PointStruct, PointsIdsList, UpsertPointsBuilder, VectorParamsBuilder, VectorsConfigBuilder,
    },
    Qdrant,
};

use teloxide::{net::Download, requests::Requester, types::Message};

use crate::bot::Bot;
use crate::{control::refresh_meme_control_msg, openai::OpenAi};

#[derive(FromQueryResult)]
struct TgUseOnlyMemeId {
    chosen_meme_id: i32,
}

#[derive(Clone)]
pub struct Storage {
    dc: DatabaseConnection,
    qd: Arc<Qdrant>,
    bot: Bot,
    openai: Arc<OpenAi>,
}

impl Storage {
    pub async fn new(bot: Bot, openai: Arc<OpenAi>) -> Result<Self> {
        let db_url = std::env::var("DATABASE_URL")?;

        let mut conn_options = ConnectOptions::new(db_url);
        conn_options.sqlx_logging_level(LevelFilter::Debug);
        conn_options.sqlx_logging(true);

        let dc = Database::connect(conn_options).await?;
        Migrator::up(&dc, None).await?;

        let qd = Arc::new(Qdrant::from_url("http://127.0.0.1:6334").build()?);

        let storage = Self {
            dc,
            qd,
            bot,
            openai,
        };
        storage.create_indexes().await?;

        Ok(storage)
    }

    /// Create qdrant index if it doesn't exist
    async fn create_indexes(&self) -> Result<()> {
        if !self.qd.collection_exists("memexpert").await? {
            let mut vectors_config = VectorsConfigBuilder::default();
            vectors_config.add_named_vector_params(
                "text-dense",
                VectorParamsBuilder::new(3072, Distance::Cosine),
            );

            self.qd
                .create_collection(
                    CreateCollectionBuilder::new("memexpert").vectors_config(vectors_config),
                )
                .await?;
        }

        Ok(())
    }

    /// Drop qdrant index and recreate it
    pub async fn reindex_all(&self) -> Result<()> {
        self.create_indexes().await?;
        self.qd.delete_collection("memexpert").await?;
        self.create_indexes().await?;

        for (meme, translations) in Memes::find()
            .find_with_related(Translations)
            .all(&self.dc)
            .await?
        {
            time::sleep(time::Duration::from_millis(100)).await;
            self.update_meme_in_qd(&meme, &translations).await?;
        }

        Ok(())
    }

    pub async fn refresh_all_control_messages(&self, bot: &Bot) -> Result<()> {
        for (meme, translations) in Memes::find()
            .find_with_related(Translations)
            .all(&self.dc)
            .await?
        {
            if let Some(new_msg) = refresh_meme_control_msg(bot, &meme, &translations).await? {
                let mut active = meme.into_active_model();
                active.control_message_id = ActiveValue::set(new_msg.id.0);
                active.save(&self.dc).await?;
            }
            time::sleep(time::Duration::from_secs(12)).await;
        }

        Ok(())
    }

    /// Create, update or delete meme in qdrant index
    async fn update_meme_in_qd(
        &self,
        meme: &memes::Model,
        translations: &[translations::Model],
    ) -> Result<()> {
        if meme.publish_status == PublishStatus::Published {
            let text_embedding = self
                .openai
                .gen_meme_text_embedding(meme, translations)
                .await?;

            self.qd
                .upsert_points(
                    UpsertPointsBuilder::new(
                        "memexpert",
                        vec![PointStruct::new(
                            u64::try_from(meme.id)?,
                            [("text-dense".to_owned(), text_embedding)]
                                .into_iter()
                                .collect::<HashMap<_, _>>(),
                            Payload::new(),
                        )],
                    )
                    .wait(true),
                )
                .await?;
        } else {
            self.qd
                .delete_points(
                    DeletePointsBuilder::new("memexpert")
                        .points(PointsIdsList {
                            ids: vec![u64::try_from(meme.id)?.into()],
                        })
                        .wait(true),
                )
                .await?;
        }

        Ok(())
    }

    /// Refresh control message, update meme in qdrant index and load files from Telegram
    async fn commit_meme_edition(
        &self,
        trans: DatabaseTransaction,
        meme_id: i32,
    ) -> Result<Option<Message>> {
        // Load final meme version
        let (meme, translations) = Memes::find_by_id(meme_id)
            .find_with_related(Translations)
            .all(&trans)
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
            .save(&trans)
            .await?;
        }
        self.update_meme_in_qd(&meme, &translations).await?;

        self.load_tg_file(&meme.tg_id, meme.content_length.try_into()?)
            .await?;
        self.load_tg_file(&meme.thumb_tg_id, meme.thumb_content_length.try_into()?)
            .await?;

        trans.commit().await?;

        Ok(control_msg)
    }

    pub async fn update_meme(
        &self,
        mut meme: memes::ActiveModel,
        translations: Vec<translations::ActiveModel>,
        updated_by: i64,
    ) -> Result<()> {
        ensure!(meme.id.is_unchanged());
        let meme_id = meme.id.clone().unwrap();

        for translation in &translations {
            ensure!(translation.meme_id.is_unchanged());
            ensure!(translation.meme_id.clone().unwrap() == meme_id);
        }

        let trans = self.dc.begin().await?;

        let prev_meme_version = Memes::find_by_id(meme_id)
            .one(&trans)
            .await?
            .context("meme not found")?;

        if meme.slug.is_set() {
            let new_slug = meme.slug.clone().unwrap();
            if prev_meme_version.slug != new_slug {
                self.bruteforce_available_slug(&trans, &mut meme).await?;

                SlugRedirects::insert(slug_redirects::ActiveModel {
                    slug: ActiveValue::set(prev_meme_version.slug.clone()),
                    meme_id: ActiveValue::set(meme_id),
                })
                .on_conflict(
                    OnConflict::column(slug_redirects::Column::Slug)
                        .update_column(slug_redirects::Column::MemeId)
                        .to_owned(),
                )
                .exec(&trans)
                .await?;
            }
        }

        meme.last_edited_by = ActiveValue::set(updated_by);
        meme.last_edition_time = ActiveValue::set(Utc::now().naive_utc());
        meme.save(&trans).await?;

        for translation in translations {
            translation.save(&trans).await?;
        }

        self.commit_meme_edition(trans, meme_id).await?;

        Ok(())
    }

    /// Create meme with translation
    pub async fn create_meme(
        &self,
        mut meme: memes::ActiveModel,
        mut translation: translations::ActiveModel,
    ) -> Result<Message> {
        let trans = self.dc.begin().await?;

        self.bruteforce_available_slug(&trans, &mut meme).await?;

        meme.control_message_id = ActiveValue::set(-1);
        let meme = Memes::insert(meme)
            .exec_with_returning(&trans)
            .await
            .unwrap();

        translation.meme_id = ActiveValue::set(meme.id);
        Translations::insert(translation).exec(&trans).await?;

        let control_msg = self
            .commit_meme_edition(trans, meme.id)
            .await?
            .context("must create control message")?;

        Ok(control_msg)
    }

    pub async fn load_meme_with_translations_by_slug(
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

    pub async fn load_meme_with_translations_by_id(
        &self,
        id: i32,
    ) -> Result<Option<(memes::Model, Vec<translations::Model>)>> {
        Ok(Memes::find_by_id(id)
            .find_with_related(Translations)
            .all(&self.dc)
            .await?
            .into_iter()
            .next())
    }

    pub async fn load_meme_by_tg_unique_id(
        &self,
        tg_unique_id: &str,
    ) -> Result<Option<memes::Model>> {
        Ok(Memes::find()
            .filter(memes::Column::TgUniqueId.eq(tg_unique_id))
            .one(&self.dc)
            .await?)
    }

    async fn bruteforce_available_slug(
        &self,
        trans: &impl ConnectionTrait,
        meme: &mut memes::ActiveModel,
    ) -> Result<()> {
        ensure!(meme.slug.is_set());
        let new_slug = meme.slug.clone().unwrap();

        if Memes::find()
            .filter(memes::Column::Slug.eq(&new_slug))
            .one(trans)
            .await?
            .is_some()
        {
            let mut i = 1u32;
            while Memes::find()
                .filter(memes::Column::Slug.eq(format!("{new_slug}-{i}")))
                .one(trans)
                .await?
                .is_some()
            {
                i += 1;
            }
            meme.slug = ActiveValue::set(format!("{new_slug}-{i}"));
        }

        Ok(())
    }

    /// Get most popular memes
    pub async fn popular_memes(&self, limit: u64) -> Result<Vec<memes::Model>> {
        let ids: Vec<_> = TgUses::find()
            .filter(tg_uses::Column::ChosenMemeId.is_not_null())
            .group_by(tg_uses::Column::ChosenMemeId)
            .order_by(tg_uses::Column::Id.count(), Order::Desc)
            .limit(limit * 2)
            .select_only()
            .column(tg_uses::Column::ChosenMemeId)
            .into_model::<TgUseOnlyMemeId>()
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|m| m.chosen_meme_id)
            .collect();

        self.memes_by_ids(&ids, limit as usize).await
    }

    async fn memes_by_ids(&self, ids: &[i32], limit: usize) -> Result<Vec<memes::Model>> {
        let memes = Memes::find()
            .filter(memes::Column::Id.is_in(ids.iter().cloned()))
            .filter(memes::Column::PublishStatus.eq(PublishStatus::Published))
            .order_by_asc(memes::Column::Id)
            .all(&self.dc)
            .await?;

        Ok(ids
            .iter()
            .unique()
            .filter_map(|i| {
                if let Ok(idx) = memes.binary_search_by_key(i, |m| m.id) {
                    Some(memes[idx].clone())
                } else {
                    None
                }
            })
            .take(limit)
            .collect())
    }

    pub async fn recent_memes(&self, user_id: i64, limit: u64) -> Result<Vec<memes::Model>> {
        let ids: Vec<_> = TgUses::find()
            .filter(tg_uses::Column::ChosenMemeId.is_not_null())
            .filter(tg_uses::Column::UserId.eq(user_id))
            .order_by(tg_uses::Column::Id, Order::Desc)
            .limit(limit * 2)
            .select_only()
            .column(tg_uses::Column::ChosenMemeId)
            .into_model::<TgUseOnlyMemeId>()
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|m| m.chosen_meme_id)
            .collect();

        self.memes_by_ids(&ids, limit as usize).await
    }

    pub async fn similar_memes(&self, meme_id: i32, limit: u64) -> Result<Vec<memes::Model>> {
        let ids: Vec<_> = self
            .qd
            .query(
                QueryPointsBuilder::new("memexpert")
                    .query(QdQuery::new_nearest(meme_id as u64))
                    .using("text-dense")
                    .limit(limit * 2),
            )
            .await?
            .result
            .into_iter()
            .map(
                |r| match r.id.unwrap_or_default().point_id_options.unwrap() {
                    PointIdOptions::Num(n) => n as i32,
                    PointIdOptions::Uuid(_) => -1,
                },
            )
            .collect();

        self.memes_by_ids(&ids, limit as usize).await
    }

    /// Search most relevant memes by query
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

        Ok(if query.is_empty() {
            let recent = self.recent_memes(user_id, 50).await?;
            let popular = self.popular_memes(50).await?;

            recent
                .into_iter()
                .map(|m| (m, 'r', tg_use.id))
                .chain(popular.into_iter().map(|m| (m, 'p', tg_use.id)))
                .unique_by(|m| m.0.id)
                .take(50)
                .collect()
        } else {
            let text_embedding = self.openai.text_embedding(query).await?;

            let qd_results = self
                .qd
                .query(
                    QueryPointsBuilder::new("memexpert")
                        .add_prefetch(
                            PrefetchQueryBuilder::default()
                                .query(QdQuery::new_nearest(text_embedding))
                                .using("text-dense")
                                .limit(100u32),
                        )
                        .query(QdQuery::new_fusion(Fusion::Rrf))
                        .limit(100),
                )
                .await?;

            let mut qd_ids_scores = qd_results
                .result
                .into_iter()
                .map(|r| {
                    (
                        match r.id.unwrap_or_default().point_id_options.unwrap() {
                            PointIdOptions::Num(n) => n as i32,
                            PointIdOptions::Uuid(_) => -1,
                        },
                        r.score,
                        'q',
                    )
                })
                .collect_vec();

            qd_ids_scores.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let ids = qd_ids_scores
                .into_iter()
                .map(|i| (i.0, i.2))
                .filter(|(i, _)| *i != -1)
                .collect_vec();

            let memes = Memes::find()
                .filter(memes::Column::Id.is_in(ids.iter().map(|i| i.0)))
                .filter(memes::Column::PublishStatus.eq(PublishStatus::Published))
                .order_by_asc(memes::Column::Id)
                .all(&self.dc)
                .await?;

            ids.into_iter()
                .filter_map(|i| {
                    if let Ok(idx) = memes.binary_search_by_key(&i.0, |m| m.id) {
                        Some((memes[idx].clone(), i.1, tg_use.id))
                    } else {
                        None
                    }
                })
                .take(50)
                .collect()
        })
    }

    /// Get the new slug by the old slug
    pub async fn get_slug_redirect(&self, slug: &str) -> Result<Option<String>> {
        if let Some(meme_id) = SlugRedirects::find_by_id(slug)
            .one(&self.dc)
            .await?
            .map(|r| r.meme_id)
        {
            Ok(Memes::find_by_id(meme_id)
                .one(&self.dc)
                .await?
                .map(|m| m.slug))
        } else {
            Ok(None)
        }
    }

    /// Get all memes with translations
    pub async fn all_memes_with_translations(
        &self,
    ) -> Result<Vec<(memes::Model, Vec<translations::Model>)>> {
        let memes = Memes::find()
            .order_by_asc(memes::Column::Id)
            .find_with_related(Translations)
            .all(&self.dc)
            .await?;
        Ok(memes)
    }

    /// Save chosen in Telegram inline mode meme into database
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

    /// Save web visit into database
    pub async fn save_web_visit(&self, visit: web_visits::ActiveModel) -> Result<()> {
        WebVisits::insert(visit).exec(&self.dc).await?;
        Ok(())
    }

    /// Load and cache into database file from Telegram by its id
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
