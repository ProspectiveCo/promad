use std::{cell::RefCell, error::Error, sync::Arc};

use nomad::{repo::postgres::PostgresNomadRepo, Migration, MigrationUI, Migrator};
use once_cell::sync::Lazy;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres};
use testcontainers::{clients, Container};

use self::postgres::PostgresImage;

pub mod postgres;

#[macro_export]
macro_rules! create_migration {
    ($name:ident, $name_str:expr, $up_sql:expr, $down_sql:expr) => {{
        struct $name;

        #[async_trait::async_trait]
        impl Migration<sqlx::Postgres> for $name {
            fn name(&self) -> &'static str {
                $name_str.into()
            }

            async fn up(
                &self,
                _read: &mut <sqlx::Postgres as Database>::Connection,
                write: &mut <sqlx::Postgres as Database>::Connection,
            ) -> crate::error::Result<()> {
                tracing::info!("Running up migration {}", self.name());
                tracing::info!("Running SQL: {}", $up_sql);
                sqlx::query($up_sql).execute(write).await?;
                Ok(())
            }

            async fn down(
                &self,
                _read: &mut <sqlx::Postgres as Database>::Connection,
                write: &mut <sqlx::Postgres as Database>::Connection,
            ) -> crate::error::Result<()> {
                tracing::info!("Running down migration {}", self.name());
                tracing::info!("Running SQL: {}", $down_sql);
                sqlx::query($down_sql).execute(write).await?;
                Ok(())
            }
        }

        || Box::new($name {}) as Box<dyn Migration<sqlx::Postgres>>
    }};
}

static DOCKER: Lazy<clients::Cli> = Lazy::new(|| clients::Cli::default());

pub struct TestHarness<'a> {
    pub pool: PgPool,
    pub pgsql: Container<'a, PostgresImage>,
    pub uis: Arc<RefCell<Vec<MockUI>>>,
    pub migrator: Migrator<Postgres>,
    pub repo: PostgresNomadRepo,
}

impl TestHarness<'_> {
    pub fn get_mock_uis(&self) -> Vec<MockUI> {
        self.uis.borrow().clone()
    }
}

pub async fn make_test_harness() -> Result<TestHarness<'static>, Box<dyn Error>> {
    let pgsql = DOCKER.run(PostgresImage::default());
    let port = pgsql.get_host_port_ipv4(5432);
    let pool = PgPoolOptions::new()
        // .max_connections(10)
        .connect(&format!(
            "postgres://postgres:postgres@localhost:{}/postgres",
            port
        ))
        .await?;
    let uis = Arc::new(RefCell::new(Vec::new()));
    let uis_clone = uis.clone();
    let factory: Box<dyn Fn(&[(i64, &dyn Migration<Postgres>)]) -> Box<dyn MigrationUI>> =
        Box::new(move |_migrations| {
            let ui = MockUI {
                messages: Arc::new(RefCell::new(Vec::new())),
            };
            uis_clone.clone().borrow_mut().push(ui.clone());
            Box::new(ui)
        });
    let migrator = Migrator::create_with_ui(pool.clone(), factory);
    Ok(TestHarness {
        pool,
        pgsql,
        migrator,
        uis,
        repo: PostgresNomadRepo,
    })
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MockUICommands {
    Start(usize, nomad::Direction),
    Finish(usize),
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockUI {
    messages: Arc<RefCell<Vec<MockUICommands>>>,
}

impl MockUI {
    pub fn messages(&self) -> Vec<MockUICommands> {
        self.messages.borrow().clone()
    }
}

impl MigrationUI for MockUI {
    fn start(&self, idx: usize, direction: &nomad::Direction) {
        self.messages
            .borrow_mut()
            .push(MockUICommands::Start(idx, direction.clone()));
    }

    fn complete(&self) {
        self.messages.borrow_mut().push(MockUICommands::Complete);
    }

    fn finish(&self, idx: usize) {
        self.messages.borrow_mut().push(MockUICommands::Finish(idx));
    }
}
