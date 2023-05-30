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

use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard};

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
    #[error("Failed to acquire cache log")]
    LockError(String),
}

impl<'a, T> From<PoisonError<RwLockReadGuard<'a, T>>> for Error {
    fn from(e: PoisonError<RwLockReadGuard<'a, T>>) -> Self {
        Error::LockError(e.to_string())
    }
}

impl<'a, T> From<PoisonError<RwLockWriteGuard<'a, T>>> for Error {
    fn from(e: PoisonError<RwLockWriteGuard<'a, T>>) -> Self {
        Error::LockError(e.to_string())
    }
}

pub type Result<A> = std::result::Result<A, Error>;
