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

#![doc = include_str!("../README.md")]

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use repo::CachedPromadRepo;
use std::{collections::HashSet, time::Duration};

use once_cell::sync::Lazy;

use async_trait::async_trait;
use chrono::Utc;

#[cfg(feature = "postgres")]
use repo::postgres::PostgresPromadRepo;
#[cfg(feature = "postgres")]
use sqlx::Postgres;

use colored::Colorize;
use sqlx::{Connection, Database, Pool};
use std::io::Write;

pub mod cli;
pub mod error;
pub mod repo;

use crate::repo::{PromadRepo, PromadRow};

/// Good default for migration names.
#[macro_export]
macro_rules! file_basename {
    () => {{
        use std::path::Path;

        let full_path = file!();
        let path = Path::new(full_path);
        let basename = path.file_name().unwrap().to_str().unwrap();
        basename
    }};
}

/// Trait representing a migration. Up/Down each get separate connections for read/write.
/// The idea behind this is that users can stream data from the read connection and write it
/// to the write connection. This is useful for migrating data in blob columns whose schemas
/// aren't managed by SQL.
///
/// One caveat is that the read connection and write connection are not part of the same transaction.
/// This means that the safest way to run migrations with both read/write usage is to take the database
/// offline before running it. If you don't need read/write split, just use the write connection so that
/// everything occurs in the same txn.
#[async_trait]
pub trait Migration<DB: Database>: Send + Sync {
    fn name(&self) -> &'static str;
    /// Runs the migration. Note that any stdout will be captured until the migration is complete.
    /// Then all of the captured stdout text is printed to the console.
    async fn up(
        &self,
        read: &mut <DB as Database>::Connection,
        write: &mut <DB as Database>::Connection,
    ) -> crate::error::Result<()>;
    /// Reverts the migration. Note that any stdout will be captured until the migration is complete.
    /// Then all of the captured stdout text is printed to the console.
    async fn down(
        &self,
        read: &mut <DB as Database>::Connection,
        write: &mut <DB as Database>::Connection,
    ) -> crate::error::Result<()>;
}

/// Contains the migrations and logic for managing the migrations table,
/// handling txn, and ensuring integrity of the migrations.
pub struct Migrator<DB: Database> {
    pub(crate) migrations: Vec<Box<dyn Migration<DB>>>,
    pub(crate) pool: Pool<DB>,
    pub(crate) repo: Box<dyn PromadRepo<DB>>,
    pub(crate) ui_factory: Box<dyn Fn(&[(i64, &dyn Migration<DB>)]) -> Box<dyn MigrationUI>>,
}

/// Used for representing the status of a migration to the CLI frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMigration {
    name: &'static str,
    run_at: Option<chrono::DateTime<Utc>>,
}

static DEFAULT_PROGRESS_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::default_spinner()
        .tick_chars("◐◓◑◒ ")
        .template("{spinner:.dim.bold} {prefix:.bold.dim} {msg}")
        .unwrap()
});

/// Manage the UI for migrations. This is used to show progress bars
/// and other information to the user.
pub trait MigrationUI {
    /// Start a migration. This is called before the migration is run.
    /// The index is used to lookup which migration is running.
    fn start(&self, idx: usize, direction: &Direction);
    /// Finish a migration. This is called after the migration is run.
    fn finish(&self, idx: usize);
    /// Called at the end if any migrations ran. Just used to indicate
    /// to the user that their actions all completed successfully.
    fn complete(&self);
}

/// Interactive UI that uses indicatif to show pretty progress bars.
/// This also redirects stdout to a buffer so that the progress bars
/// aren't broken by stdout output. It's later printed to the screen
/// when migrations are complete.
///
/// This means that it's not thread safe as multiple instances will
/// all try to redirect stdout and step on each other.
pub struct InteractiveMigrationUI {
    _multi_progress: MultiProgress,
    _redirector: gag::Hold,
    progress_bars: Vec<ProgressBar>,
}

impl InteractiveMigrationUI {
    fn new<DB: Database>(migrations: &[(i64, &dyn Migration<DB>)]) -> Box<dyn MigrationUI> {
        let redirector = gag::Hold::stdout().unwrap();
        let multi_progress = MultiProgress::new();
        let migrations_len = migrations.len();
        let progress_bars = migrations
            .iter()
            .enumerate()
            .map(|(idx, (_i, migration))| {
                let progress = multi_progress
                    .add(ProgressBar::new_spinner())
                    .with_style((*DEFAULT_PROGRESS_STYLE).clone());
                progress.set_prefix(format!(
                    "[{}/{migrations_len}] {}",
                    idx + 1,
                    migration.name().bold().dimmed()
                ));
                progress.set_message("Queued");
                progress
            })
            .collect::<Vec<_>>();
        Box::new(InteractiveMigrationUI {
            _multi_progress: multi_progress,
            _redirector: redirector,
            progress_bars,
        })
    }
}

impl MigrationUI for InteractiveMigrationUI {
    fn start(&self, idx: usize, direction: &Direction) {
        let progress = &self.progress_bars[idx];
        progress.enable_steady_tick(Duration::from_millis(100));
        match direction {
            Direction::Up => {
                progress.set_message("Running up migration");
            }
            Direction::Down => {
                progress.set_message("Running down migration");
            }
        }
    }

    fn finish(&self, idx: usize) {
        let progress = &self.progress_bars[idx];
        progress.set_message("✓".green().to_string());
        progress.finish();
    }

    fn complete(&self) {
        // Required because indicatif doesn't write a newline after
        // everything is done :(
        write!(std::io::stderr(), "\n").unwrap();
        println!("✨ All migrations completed");
    }
}

/// Used to indicate whether we're running the up or down migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

pub trait HasPromadRepo: Database {
    type Repo: PromadRepo<Self>;
}

impl HasPromadRepo for Postgres {
    type Repo = PostgresPromadRepo;
}

impl<DB: Database + HasPromadRepo> Migrator<DB> {
    /// Create a Migrator with an interactive UI that isn't thread safe
    /// due to stdout being redirected while executing migrations.
    pub fn create(pool: Pool<DB>) -> Self {
        let cached = CachedPromadRepo::<DB, <DB as HasPromadRepo>::Repo>::new();
        Self {
            migrations: vec![],
            pool,
            repo: Box::new(cached),
            ui_factory: Box::new(InteractiveMigrationUI::new),
        }
    }

    /// Create a UI with a custom UI factory.
    /// This is useful for testing or using a non-interactive
    /// UI that's thread-safe.
    pub fn create_with_ui(
        pool: Pool<DB>,
        ui_factory: Box<dyn Fn(&[(i64, &dyn Migration<DB>)]) -> Box<dyn MigrationUI>>,
    ) -> Self {
        let cached = CachedPromadRepo::<DB, <DB as HasPromadRepo>::Repo>::new();
        Self {
            migrations: vec![],
            pool,
            repo: Box::new(cached),
            ui_factory,
        }
    }
}

impl<DB: Database> Migrator<DB> {
    /// Add a single migration to the migrator.
    pub fn add_migration(&mut self, migration: Box<dyn Migration<DB>>) {
        self.migrations.push(migration);
    }

    /// Add multiple migrations to the migrator.
    pub fn add_migrations(&mut self, migrations: Vec<Box<dyn Migration<DB>>>) {
        self.migrations.extend(migrations);
    }

    /// Remove a migration from the migrator by name.
    pub fn remove_migration(&mut self, name: &str) {
        self.migrations.retain(|x| x.name() != name);
    }

    /// Remove all migrations from the migrator.
    pub fn remove_all_migrations(&mut self) {
        self.migrations.clear();
    }

    /// Applies migrations up to and including the migration with the given name.
    pub async fn apply_to_inclusive(&self, up_to_name: &str) -> crate::error::Result<()> {
        self.init_sql().await?;
        self.validate_all().await?;
        if !self
            .migrations
            .iter()
            .map(|x| x.name())
            .any(|x| x == up_to_name)
        {
            return Err(error::Error::NoSuchMigration(up_to_name.to_string()));
        }

        let unapplied_migrations = self.find_unapplied().await?;

        let mut migrations_to_run = Vec::new();

        for (ordering_key, unapplied) in unapplied_migrations.into_iter() {
            migrations_to_run.push((ordering_key, unapplied));
            // self.apply_one_internal(unapplied, idx as i64).await?;
            if unapplied.name() == up_to_name {
                break;
            }
        }

        self.apply_migrations(migrations_to_run, Direction::Up)
            .await?;
        Ok(())
    }

    /// Find all unapplied migrations from the tracking table.
    async fn find_unapplied(&self) -> crate::error::Result<Vec<(i64, &dyn Migration<DB>)>> {
        let mut read = self.pool.acquire().await?;
        let applied_names = self
            .repo
            .get_all(&mut read)
            .await?
            .into_iter()
            .map(|x| x.name)
            .collect::<HashSet<_>>();

        Ok(self
            .migrations
            .iter()
            .map(|x| &**x)
            .enumerate()
            .map(|(x, y)| (x as i64, y))
            .filter(|(_, x)| !applied_names.contains(x.name()))
            .collect())
    }

    /// Apply all migrations passed using either up/down script while
    /// keeping the UI up to date with the progress.
    async fn apply_migrations(
        &self,
        migrations: Vec<(i64, &dyn Migration<DB>)>,
        direction: Direction,
    ) -> crate::error::Result<()> {
        let ui = (*self.ui_factory)(&migrations);

        for (idx, (ordering_key, migration)) in migrations.iter().enumerate() {
            ui.start(idx, &direction);
            match &direction {
                Direction::Up => {
                    self.apply_one_internal(*migration, *ordering_key).await?;
                }
                Direction::Down => {
                    self.revert_one_internal(*migration).await?;
                }
            }
            ui.finish(idx);
        }

        if migrations.len() > 0 {
            ui.complete();
        }

        Ok(())
    }

    /// Apply all migrations that haven't been applied yet.
    pub async fn apply_all(&self) -> crate::error::Result<()> {
        self.init_sql().await?;
        self.validate_all().await?;

        let unapplied_migrations = self.find_unapplied().await?;
        self.apply_migrations(unapplied_migrations, Direction::Up)
            .await?;
        Ok(())
    }

    /// Revet all migrations that have been applied.
    pub async fn revert_all(&self) -> crate::error::Result<()> {
        self.init_sql().await?;
        self.validate_all().await?;

        let mut conn = self.pool.acquire().await?;
        let mut txn = conn.begin().await?;
        let applied_migrations = self.repo.get_all(&mut txn).await?;

        let to_revert = applied_migrations
            .iter()
            .rev()
            .map(|x| (x.ordering_key, &*self.migrations[x.ordering_key as usize]))
            .collect::<Vec<_>>();

        self.apply_migrations(to_revert, Direction::Down).await?;
        Ok(())
    }

    /// List all migration with data about whether they've been applied or not and when.
    pub async fn list_migrations(&self) -> crate::error::Result<Vec<UiMigration>> {
        self.init_sql().await?;
        self.validate_all().await?;

        let mut read = self.pool.acquire().await?;
        let applied_migrations = self.repo.get_all(&mut read).await?;

        Ok(self
            .migrations
            .iter()
            .map(|x| Some(x))
            .zip(
                applied_migrations
                    .into_iter()
                    .map(Some)
                    .chain(std::iter::repeat(None)),
            )
            .filter_map(|(x, y)| match (x, y) {
                (Some(x), Some(y)) => Some(UiMigration {
                    name: x.name().into(),
                    run_at: Some(y.created_at),
                }),
                (Some(x), None) => Some(UiMigration {
                    name: x.name().into(),
                    run_at: None,
                }),
                _ => None,
            })
            .collect::<Vec<_>>())
    }

    /// Reverts all migrations up to and including the one with the given name.
    pub async fn revert_to_inclusive(&self, name: &str) -> crate::error::Result<()> {
        self.init_sql().await?;
        self.validate_all().await?;
        if !self.migrations.iter().map(|x| x.name()).any(|x| x == name) {
            return Err(error::Error::NoSuchMigration(name.to_string()));
        }

        let mut conn = self.pool.acquire().await?;
        let mut txn = conn.begin().await?;
        let applied_migrations = self.repo.get_all(&mut txn).await?;

        let mut to_revert = Vec::new();

        for migration in applied_migrations.iter().rev() {
            to_revert.push((
                migration.ordering_key,
                &*self.migrations[migration.ordering_key as usize],
            ));
            if migration.name == name {
                break;
            }
        }

        self.apply_migrations(to_revert, Direction::Down).await?;
        Ok(())
    }

    /// Runs the database specific SQL to initialize the tracking table.
    async fn init_sql(&self) -> crate::error::Result<()> {
        let mut write = self.pool.acquire().await?;
        let mut txn = write.begin().await?;
        self.repo.init(&mut txn).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Check that the migrations given pass all validation rule.
    async fn validate_all(&self) -> crate::error::Result<()> {
        self.validate_name_uniqueness()?;
        self.validate_db_against_local().await?;
        Ok(())
    }

    /// Validate that migration names are unique.
    fn validate_name_uniqueness(&self) -> crate::error::Result<()> {
        let mut names = std::collections::HashSet::new();
        for migration in &self.migrations {
            if names.contains(&migration.name()) {
                return Err(error::Error::DuplicateMigrationName(
                    migration.name().to_string(),
                ));
            }
            names.insert(migration.name());
        }
        Ok(())
    }

    /// Validate that the migrations in the database match the ones in the local directory.
    async fn validate_db_against_local<'a>(&self) -> crate::error::Result<()> {
        let mut read = self.pool.acquire().await?;
        let previously_applied = self
            .repo
            .get_all(&mut read)
            .await?
            .into_iter()
            .collect::<Vec<_>>();
        if self.migrations.len() < previously_applied.len() {
            return Err(error::Error::DeletedMigrations {
                db_migration_count: previously_applied.len(),
                local_migration_count: self.migrations.len(),
            });
        }

        for (i, row) in previously_applied.iter().enumerate() {
            let local_migration = &*self.migrations[i];
            if local_migration.name() != row.name {
                return Err(error::Error::HistoryMigrationMismatch {
                    remote_name: row.name.clone(),
                    local_name: local_migration.name().to_string(),
                });
            }
        }
        Ok(())
    }

    /// Write to the tracking table that the migration has been applied.
    async fn record_completion(
        &self,
        write: &mut <DB as Database>::Connection,
        migration: &dyn Migration<DB>,
        ordering_key: i64,
    ) -> crate::error::Result<()> {
        self.repo
            .insert(
                &PromadRow {
                    name: migration.name().to_string(),
                    ordering_key,
                    created_at: Utc::now(),
                },
                write,
            )
            .await?;
        Ok(())
    }

    /// Helper for applying a single migration in a transaction.
    async fn apply_one_internal(
        &self,
        migration: &dyn Migration<DB>,
        ordering_key: i64,
    ) -> crate::error::Result<()> {
        let mut read = self.pool.acquire().await?;
        let mut write = self.pool.acquire().await?;

        let mut r = read.begin().await?;
        self.repo.set_read_only(&mut r).await?;
        let mut w = write.begin().await?;
        migration.up(&mut r, &mut *w).await?;
        self.record_completion(&mut *w, migration, ordering_key)
            .await?;
        w.commit().await?;

        Ok(())
    }

    // Helper for reverting a single migration in a transaction.
    async fn revert_one_internal(&self, migration: &dyn Migration<DB>) -> crate::error::Result<()> {
        let mut read = self.pool.acquire().await?;
        let mut write = self.pool.acquire().await?;

        let mut r = read.begin().await?;
        self.repo.set_read_only(&mut r).await?;
        let mut w = write.begin().await?;
        migration.down(&mut r, &mut *w).await?;
        self.repo.delete(migration.name().into(), &mut *w).await?;
        w.commit().await?;

        Ok(())
    }
}
