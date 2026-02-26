pub mod entities;
mod roast_repository;
mod user_repository;
mod vote_repository;

pub use roast_repository::RoastRepository;
pub use user_repository::UserRepository;
pub use vote_repository::VoteRepository;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr, Statement};
use std::time::Duration;

pub async fn create_connection(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(database_url);
    opt.max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(600))
        .sqlx_logging(false);

    Database::connect(opt).await
}

pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Read and execute migration file
    let migration = include_str!("../../../../migrations/001_initial.sql");

    // Split by semicolons and execute each statement
    for statement in migration.split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            // Ignore errors for CREATE TABLE IF NOT EXISTS style operations
            let _ = db
                .execute(Statement::from_string(
                    sea_orm::DatabaseBackend::Postgres,
                    statement.to_string(),
                ))
                .await;
        }
    }

    Ok(())
}
