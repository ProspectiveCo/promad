# <i>Pro</i>mad

<p align="center"><img src="promad.png" width="25%" alt="Promad logo"></p>

Migration tool for SQLx that allows for arbitrary SQL/Rust execution within a transaction on the write connection.

## Features
* Rust code mixed with SQL in migrations.
* Scan migrations across a table with blob data using Rust.
* Embeddable CLI.

## Example

```rust
# #[tokio::main]
# async fn main() {
use std::borrow::Cow;
use async_trait::async_trait;
use promad::{file_basename, Migration, Migrator, error::{Error, Result}};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Executor, Row};
use testcontainers::{clients, Container};

pub struct FirstMigration;

#[async_trait]
impl Migration<Postgres> for FirstMigration {
    fn name(&self) -> &'static str {
        file_basename!()
    }

    async fn up(
        &self,
        _read: &mut sqlx::PgConnection,
        write: &mut sqlx::PgConnection,
    ) -> Result<()> {
        sqlx::query("CREATE TABLE test (id INT PRIMARY KEY)")
            .execute(write)
            .await?;
        Ok(())
    }

    async fn down(
        &self,
        _read: &mut sqlx::PgConnection,
        write: &mut sqlx::PgConnection,
    ) -> Result<()> {
        sqlx::query("DROP TABLE test")
            .execute(write)
            .await?;
        Ok(())
    }
}

let docker = clients::Cli::default();
let pgsql = docker.run(testcontainers::images::postgres::Postgres::default());
let port = pgsql.get_host_port_ipv4(5432);
let pool = PgPoolOptions::new()
    .connect(&format!(
        "postgres://postgres:postgres@localhost:{}/postgres",
        port
    ))
    .await.unwrap();

let mut migrator = Migrator::create(pool.clone());
migrator.add_migration(Box::new(FirstMigration));
migrator.apply_all().await.unwrap();

// Check that the table exists.
let mut conn = pool.acquire().await.unwrap();

let row = sqlx::query("SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'test')")
    .fetch_one(conn.as_mut())
    .await.unwrap();

assert!(row.get::<bool, _>(0));

migrator.revert_all().await.unwrap();

let row = sqlx::query("SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'test')")
    .fetch_one(conn.as_mut())
    .await.unwrap();

assert!(!row.get::<bool, _>(0));

# }
```