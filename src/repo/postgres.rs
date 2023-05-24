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

use std::borrow::Cow;

use async_trait::async_trait;
use sqlx::Database;
use sqlx::Postgres;

use super::NomadRepo;
use super::NomadRow;
use crate::error::Result;

const INIT_SQL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS _nomad (
        name TEXT NOT NULL PRIMARY KEY,
        ordering_key BIGINT NOT NULL,
        created_at TIMESTAMP WITH TIME ZONE NOT NULL
    );"#,
    "CREATE INDEX IF NOT EXISTS idx_nomad_ordering_key ON _nomad (ordering_key);",
];

pub struct PostgresNomadRepo;

#[async_trait]
impl NomadRepo<Postgres> for PostgresNomadRepo {
    fn new() -> Self {
        Self
    }

    async fn init<'a>(&self, conn: &'a mut <Postgres as Database>::Connection) -> Result<()> {
        for sql in INIT_SQL {
            sqlx::query(sql).execute(&mut *conn).await?;
        }
        Ok(())
    }

    async fn set_read_only<'a>(
        &self,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> Result<()> {
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(conn)
            .await?;
        Ok(())
    }

    async fn get_all<'a>(
        &self,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> Result<Vec<NomadRow>> {
        let rows = sqlx::query_as::<_, NomadRow>("SELECT * FROM _nomad ORDER BY ordering_key")
            .fetch_all(conn)
            .await?;
        Ok(rows)
    }

    async fn get<'a>(
        &self,
        name: &str,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> Result<Option<NomadRow>> {
        let row = sqlx::query_as::<_, NomadRow>("SELECT * FROM _nomad WHERE name = $1")
            .bind(name)
            .fetch_optional(conn)
            .await?;
        Ok(row)
    }

    async fn insert<'a>(
        &self,
        row: &NomadRow,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> Result<()> {
        sqlx::query("INSERT INTO _nomad (name, ordering_key, created_at) VALUES ($1, $2, $3)")
            .bind(row.name.clone())
            .bind(row.ordering_key)
            .bind(row.created_at)
            .execute(conn)
            .await?;
        Ok(())
    }

    async fn delete<'a>(
        &self,
        name: Cow<'static, str>,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> Result<()> {
        sqlx::query("DELETE FROM _nomad WHERE name = $1")
            .bind(name)
            .execute(conn)
            .await?;
        Ok(())
    }
}
