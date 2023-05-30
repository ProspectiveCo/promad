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

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use sqlx::Database;

#[cfg(feature = "postgres")]
pub mod postgres;

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct NomadRow {
    pub(crate) name: String,
    pub(crate) ordering_key: i64,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
}

/// A trait for interacting with the migrations table
/// on any supported underlying database.
#[async_trait]
pub trait NomadRepo<DB: Database>: Send + Sync {
    fn new() -> Self
    where
        Self: Sized;
    /// Creates the migrations table if it does not exist.
    async fn init<'a>(
        &self,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()>;
    /// Set the current transaction to read only.
    async fn set_read_only<'a>(
        &self,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()>;
    /// Return the rows ordered by `ordering_key`.
    async fn get_all<'a>(
        &self,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<Vec<NomadRow>>;
    /// Get specific migration by name.
    async fn get<'a>(
        &self,
        name: &str,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<Option<NomadRow>>;
    /// Insert a new migration.
    async fn insert<'a>(
        &self,
        row: &NomadRow,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()>;
    /// Remove a migration.
    async fn delete<'a>(
        &self,
        row: &'static str,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()>;
}

pub struct CachedNomadRepo<DB: Database, N: NomadRepo<DB>> {
    inner: Box<dyn NomadRepo<DB>>,
    cache: Arc<RwLock<BTreeMap<i64, NomadRow>>>,
    is_db_loaded: Arc<RwLock<bool>>,
    _marker: std::marker::PhantomData<N>,
}

#[async_trait]
impl<DB: Database, N: NomadRepo<DB> + 'static> NomadRepo<DB> for CachedNomadRepo<DB, N> {
    fn new() -> Self {
        Self {
            inner: Box::new(N::new()),
            cache: Arc::new(RwLock::new(BTreeMap::new())),
            is_db_loaded: Arc::new(RwLock::new(false)),
            _marker: Default::default(),
        }
    }

    async fn init<'a>(
        &self,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()> {
        self.inner.init(conn).await
    }

    async fn set_read_only<'a>(
        &self,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()> {
        self.inner.set_read_only(conn).await
    }

    async fn get_all<'a>(
        &self,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<Vec<NomadRow>> {
        {
            let is_db_loaded = self.is_db_loaded.read()?;
            if *is_db_loaded {
                let cache = self.cache.read()?;
                return Ok(cache.values().cloned().collect());
            }
        }

        let rows = {
            let rows = self.inner.get_all(conn).await?;
            let mut cache = self.cache.write()?;
            for row in &rows {
                cache.insert(row.ordering_key, row.clone());
            }
            rows
        };

        // Update the state to true indicating the database has been loaded into cache.
        let mut is_db_loaded = self.is_db_loaded.write()?;
        *is_db_loaded = true;

        Ok(rows)
    }

    async fn get<'a>(
        &self,
        name: &str,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<Option<NomadRow>> {
        {
            let is_db_loaded = self.is_db_loaded.read()?;
            if *is_db_loaded {
                let cache = self.cache.read()?;
                return Ok(cache.values().find(|&row| row.name == name).cloned());
            }
        }

        let row = self.inner.get(name, conn).await?;
        if let Some(ref r) = row {
            let mut cache = self.cache.write()?;
            cache.insert(r.ordering_key, r.clone());
        }

        Ok(row)
    }

    async fn insert<'a>(
        &self,
        row: &NomadRow,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()> {
        self.inner.insert(row, conn).await?;
        let mut cache = self.cache.write()?;
        cache.insert(row.ordering_key, row.clone());
        Ok(())
    }

    async fn delete<'a>(
        &self,
        name: &'static str,
        conn: &'a mut <DB as Database>::Connection,
    ) -> crate::error::Result<()> {
        self.inner.delete(name, conn).await?;
        let mut cache = self.cache.write()?;
        cache.retain(|_, row| row.name != name);
        Ok(())
    }
}
