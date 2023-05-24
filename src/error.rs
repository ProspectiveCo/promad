// ┌───────────────────────────────────────────────────────────────────────────┐
// │                                                                           │
// │  ██████╗ ██████╗  ██████╗   Copyright (C) The Prospective Company         │
// │  ██╔══██╗██╔══██╗██╔═══██╗  All Rights Reserved - April 2022              │
// │  ██████╔╝██████╔╝██║   ██║                                                │
// │  ██╔═══╝ ██╔══██╗██║   ██║  Proprietary and confidential. Unauthorized    │
// │  ██║     ██║  ██║╚██████╔╝  copying of this file, via any medium is       │
// │  ╚═╝     ╚═╝  ╚═╝ ╚═════╝   strictly prohibited.                          │
// │                                                                           │
// └───────────────────────────────────────────────────────────────────────────┘

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Sqlx error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("No such migration: {0}")]
    NoSuchMigration(String),
    #[error("{db_migration_count} migrations have been applied to the database, but {local_migration_count} migrations have been found locally")]
    DeletedMigrations {
        db_migration_count: usize,
        local_migration_count: usize,
    },
    #[error("Duplicate migration name: {0}")]
    DuplicateMigrationName(String),
    #[error(
        "The migration history shows that {remote_name} should be the next migration, but locally there is {local_name}"
    )]
    HistoryMigrationMismatch {
        remote_name: String,
        local_name: String,
    },
}

pub type Result<A> = std::result::Result<A, Error>;
