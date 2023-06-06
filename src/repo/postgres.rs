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

use async_trait::async_trait;
use sqlx::Database;
use sqlx::Postgres;

use super::PromadRepo;
use super::PromadRow;

const INIT_SQL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS _promad (
        name TEXT NOT NULL PRIMARY KEY,
        ordering_key BIGINT NOT NULL,
        created_at TIMESTAMP WITH TIME ZONE NOT NULL
    );"#,
    "CREATE INDEX IF NOT EXISTS idx_promad_ordering_key ON _promad (ordering_key);",
];

pub struct PostgresPromadRepo;

#[async_trait]
impl PromadRepo<Postgres> for PostgresPromadRepo {
    fn new() -> Self {
        Self
    }

    async fn init<'a>(
        &self,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> crate::error::Result<()> {
        for sql in INIT_SQL {
            sqlx::query(sql).execute(&mut *conn).await?;
        }
        Ok(())
    }

    async fn set_read_only<'a>(
        &self,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> crate::error::Result<()> {
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(conn)
            .await?;
        Ok(())
    }

    async fn get_all<'a>(
        &self,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> crate::error::Result<Vec<PromadRow>> {
        let rows = sqlx::query_as::<_, PromadRow>("SELECT * FROM _promad ORDER BY ordering_key")
            .fetch_all(conn)
            .await?;
        Ok(rows)
    }

    async fn get<'a>(
        &self,
        name: &str,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> crate::error::Result<Option<PromadRow>> {
        let row = sqlx::query_as::<_, PromadRow>("SELECT * FROM _promad WHERE name = $1")
            .bind(name)
            .fetch_optional(conn)
            .await?;
        Ok(row)
    }

    async fn insert<'a>(
        &self,
        row: &PromadRow,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> crate::error::Result<()> {
        sqlx::query("INSERT INTO _promad (name, ordering_key, created_at) VALUES ($1, $2, $3)")
            .bind(row.name.clone())
            .bind(row.ordering_key)
            .bind(row.created_at)
            .execute(conn)
            .await?;
        Ok(())
    }

    async fn delete<'a>(
        &self,
        name: &'static str,
        conn: &'a mut <Postgres as Database>::Connection,
    ) -> crate::error::Result<()> {
        sqlx::query("DELETE FROM _promad WHERE name = $1")
            .bind(name)
            .execute(conn)
            .await?;
        Ok(())
    }
}
