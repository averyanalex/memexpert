use sea_orm_migration::prelude::*;

use crate::m20240408_005449_init::WebVisits;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WebVisits::Table)
                    .add_column(
                        ColumnDef::new(WebVisits::IsBot)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WebVisits::Table)
                    .drop_column(WebVisits::IsBot)
                    .to_owned(),
            )
            .await
    }
}
