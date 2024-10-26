//! `SeaORM` Entity, @generated by sea-orm-codegen 1.0.1

use sea_orm::entity::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "media_type")]
pub enum MediaType {
    #[sea_orm(string_value = "animation")]
    Animation,
    #[sea_orm(string_value = "photo")]
    Photo,
    #[sea_orm(string_value = "video")]
    Video,
}
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "publish_status")]
pub enum PublishStatus {
    #[sea_orm(string_value = "draft")]
    Draft,
    #[sea_orm(string_value = "published")]
    Published,
    #[sea_orm(string_value = "trash")]
    Trash,
}
