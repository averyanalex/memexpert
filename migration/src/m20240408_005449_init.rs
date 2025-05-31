use sea_orm::{sea_query::extension::postgres::Type, EnumIter, Iterable};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(MediaType::Table)
                    .values(MediaType::iter().skip(1))
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(PublishStatus::Table)
                    .values(PublishStatus::iter().skip(1))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Memes::Table)
                    .col(
                        ColumnDef::new(Memes::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Memes::Slug).string().not_null().unique_key())
                    .col(
                        ColumnDef::new(Memes::CreationTime)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Memes::CreatedBy).big_integer().not_null())
                    .col(
                        ColumnDef::new(Memes::LastEditionTime)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Memes::LastEditedBy).big_integer().not_null())
                    .col(
                        ColumnDef::new(Memes::PublishStatus)
                            .enumeration(PublishStatus::Table, PublishStatus::iter().skip(1))
                            .not_null()
                            .default(
                                Expr::val(PublishStatus::Draft.to_string())
                                    .cast_as(PublishStatus::Table),
                            ),
                    )
                    .col(ColumnDef::new(Memes::Source).string())
                    .col(
                        ColumnDef::new(Memes::ControlMessageId)
                            .integer()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Memes::Text).string())
                    .col(
                        ColumnDef::new(Memes::MediaType)
                            .enumeration(MediaType::Table, MediaType::iter().skip(1))
                            .not_null(),
                    )
                    .col(ColumnDef::new(Memes::MimeType).string().not_null())
                    .col(ColumnDef::new(Memes::Width).integer().not_null())
                    .col(ColumnDef::new(Memes::Height).integer().not_null())
                    .col(ColumnDef::new(Memes::Duration).integer().not_null())
                    .col(
                        ColumnDef::new(Memes::TgUniqueId)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Memes::TgId).string().not_null())
                    .col(ColumnDef::new(Memes::ContentLength).integer().not_null())
                    .col(ColumnDef::new(Memes::ThumbMimeType).string().not_null())
                    .col(ColumnDef::new(Memes::ThumbWidth).integer().not_null())
                    .col(ColumnDef::new(Memes::ThumbHeight).integer().not_null())
                    .col(ColumnDef::new(Memes::ThumbTgId).string().not_null())
                    .col(
                        ColumnDef::new(Memes::ThumbContentLength)
                            .integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Translations::Table)
                    .col(ColumnDef::new(Translations::MemeId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Translations::Table, Translations::MemeId)
                            .to(Memes::Table, Memes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(Translations::Language)
                            .char_len(2)
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(Translations::MemeId)
                            .col(Translations::Language),
                    )
                    .col(ColumnDef::new(Translations::Title).string().not_null())
                    .col(ColumnDef::new(Translations::Caption).string().not_null())
                    .col(
                        ColumnDef::new(Translations::Description)
                            .string()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SlugRedirects::Table)
                    .col(
                        ColumnDef::new(SlugRedirects::Slug)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SlugRedirects::MemeId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(SlugRedirects::Table, SlugRedirects::MemeId)
                            .to(Memes::Table, Memes::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(TgUses::Table)
                    .col(
                        ColumnDef::new(TgUses::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TgUses::Timestamp)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(TgUses::UserId).big_integer().not_null())
                    .col(ColumnDef::new(TgUses::ChosenMemeId).integer())
                    .foreign_key(
                        ForeignKey::create()
                            .from(TgUses::Table, TgUses::ChosenMemeId)
                            .to(Memes::Table, Memes::Id),
                    )
                    .col(ColumnDef::new(TgUses::ChosenMemeSource).char_len(1))
                    .col(ColumnDef::new(TgUses::Query).string())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(WebVisits::Table)
                    .col(
                        ColumnDef::new(WebVisits::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WebVisits::Timestamp)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(WebVisits::UserId).char_len(8).not_null())
                    .col(ColumnDef::new(WebVisits::MemeId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(WebVisits::Table, WebVisits::MemeId)
                            .to(Memes::Table, Memes::Id),
                    )
                    .col(ColumnDef::new(WebVisits::Language).char_len(2).not_null())
                    .col(ColumnDef::new(WebVisits::Ip).string().not_null())
                    .col(ColumnDef::new(WebVisits::UserAgent).string())
                    .col(ColumnDef::new(WebVisits::Referer).string())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SlugRedirects::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WebVisits::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(TgUses::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Translations::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Memes::Table).to_owned())
            .await?;

        manager
            .drop_type(Type::drop().name(MediaType::Table).to_owned())
            .await?;
        manager
            .drop_type(Type::drop().name(PublishStatus::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden, EnumIter)]
enum MediaType {
    Table,
    Photo,
    Video,
    Animation,
}

#[derive(DeriveIden, EnumIter)]
enum PublishStatus {
    Table,
    Published,
    Draft,
    Trash,
}

#[derive(DeriveIden)]
enum Memes {
    Table,
    Id,
    Slug,
    CreationTime,
    CreatedBy,
    LastEditionTime,
    LastEditedBy,
    PublishStatus,
    Source,
    ControlMessageId,
    Text,
    MediaType,
    MimeType,
    Width,
    Height,
    Duration,
    TgUniqueId,
    TgId,
    ContentLength,
    ThumbMimeType,
    ThumbWidth,
    ThumbHeight,
    ThumbTgId,
    ThumbContentLength,
}

#[derive(DeriveIden)]
enum Translations {
    Table,
    MemeId,
    Language,
    Title,
    Caption,
    Description,
}

#[derive(DeriveIden)]
enum SlugRedirects {
    Table,
    Slug,
    MemeId,
}

#[derive(DeriveIden)]
enum TgUses {
    Table,
    Id,
    Timestamp,
    UserId,
    ChosenMemeId,
    ChosenMemeSource,
    Query,
}

#[derive(DeriveIden)]
pub enum WebVisits {
    Table,
    Id,
    Timestamp,
    UserId,
    MemeId,
    Language,
    Ip,
    UserAgent,
    Referer,
    IsBot,
}
