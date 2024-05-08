pub use sea_orm_migration::prelude::*;

mod m20240408_005449_init;
mod m20240508_214652_create_files_cache;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240408_005449_init::Migration),
            Box::new(m20240508_214652_create_files_cache::Migration),
        ]
    }
}
