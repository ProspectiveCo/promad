use promad::*;

use sqlx::Database;

use std::error::Error;

mod common;

use common::*;

#[tokio::test]
async fn test_good_migration() -> Result<(), Box<dyn Error>> {
    let migration = create_migration!(
        TestMigration,
        "test_migration",
        "CREATE TABLE test (id INT PRIMARY KEY)",
        "DROP TABLE test"
    );
    let mut env = make_test_harness().await?;
    env.migrator.add_migration(migration());
    env.migrator.apply_all().await?;

    let mock_uis = env.get_mock_uis();
    assert_eq!(
        mock_uis[0].messages(),
        vec![
            MockUICommands::Start(0, Direction::Up),
            MockUICommands::Finish(0),
            MockUICommands::Complete
        ]
    );

    let mut conn = env.pool.acquire().await?;

    sqlx::query("INSERT INTO test VALUES (1)")
        .execute(conn.as_mut())
        .await?;

    let row: (i32,) = sqlx::query_as("SELECT 1 FROM test")
        .fetch_one(conn.as_mut())
        .await?;
    assert_eq!(row.0, 1);

    env.migrator.revert_all().await?;

    let row: Result<(i32,), sqlx::Error> = sqlx::query_as("SELECT 1 FROM test")
        .fetch_one(conn.as_mut())
        .await;

    assert!(row.is_err());

    Ok(())
}

#[tokio::test]
async fn test_rename_migration_on_disk() -> Result<(), Box<dyn Error>> {
    let migration = create_migration!(
        TestMigration,
        "test_migration",
        "CREATE TABLE test (id INT PRIMARY KEY)",
        "DROP TABLE test"
    );
    let renamed = create_migration!(
        RenamedMigration,
        "renamed_migration",
        "CREATE TABLE test (id INT PRIMARY KEY)",
        "DROP TABLE test"
    );

    let mut env = make_test_harness().await?;

    env.migrator.add_migration(migration());
    env.migrator.apply_all().await?;

    let mut new_migrator = Migrator::create(env.pool.clone());
    new_migrator.add_migration(renamed());

    let res = new_migrator.list_migrations().await;

    assert!(matches!(
        res,
        Err(promad::error::Error::HistoryMigrationMismatch { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn test_duplicate_migration() -> Result<(), Box<dyn Error>> {
    let migration1 = create_migration!(
        FirstMigration,
        "duplicate_migration",
        "CREATE TABLE test1 (id INT PRIMARY KEY)",
        "DROP TABLE test1"
    );
    let migration2 = create_migration!(
        SecondMigration,
        "duplicate_migration",
        "CREATE TABLE test2 (id INT PRIMARY KEY)",
        "DROP TABLE test2"
    );
    let mut env = make_test_harness().await?;
    env.migrator.add_migration(migration1());
    env.migrator.add_migration(migration2());
    let res = env.migrator.apply_all().await;
    assert!(matches!(
        res,
        Err(crate::error::Error::DuplicateMigrationName(_))
    ));
    Ok(())
}

#[tokio::test]
async fn test_invalid_sql_command() -> Result<(), Box<dyn Error>> {
    let migration = create_migration!(
        BadMigration,
        "bad_migration",
        "CREATE TABLEX test (id INT PRIMARY KEY)", // wrong SQL command: TABLEX instead of TABLE
        "DROP TABLE test"
    );
    let mut env = make_test_harness().await?;
    env.migrator.add_migration(migration());
    let res = env.migrator.apply_all().await;
    assert!(matches!(res, Err(promad::error::Error::DatabaseError(_))));
    Ok(())
}

#[tokio::test]
async fn test_rename_table_migration() -> Result<(), Box<dyn Error>> {
    let migration1 = create_migration!(
        FirstMigration,
        "migration1",
        "CREATE TABLE test1 (id INT PRIMARY KEY)",
        "DROP TABLE test1"
    );
    let migration2 = create_migration!(
        RenameMigration,
        "rename_migration",
        "ALTER TABLE test1 RENAME TO test2",
        "ALTER TABLE test2 RENAME TO test1"
    );
    let mut env = make_test_harness().await?;
    env.migrator.add_migration(migration1());
    env.migrator.add_migration(migration2());
    env.migrator.apply_all().await?;

    let mut conn = env.pool.acquire().await?;
    let row: Result<Option<(i32,)>, sqlx::Error> = sqlx::query_as("SELECT 1 FROM test2")
        .fetch_optional(conn.as_mut())
        .await;
    assert!(row.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_apply_revert_apply_different() -> Result<(), Box<dyn Error>> {
    // create three initial migrations
    let migration1 = create_migration!(
        Migration1,
        "migration1",
        "CREATE TABLE test1 (id INT PRIMARY KEY)",
        "DROP TABLE test1"
    );
    let migration2 = create_migration!(
        Migration2,
        "migration2",
        "CREATE TABLE test2 (id INT PRIMARY KEY)",
        "DROP TABLE test2"
    );
    let migration3 = create_migration!(
        Migration3,
        "migration3",
        "CREATE TABLE test3 (id INT PRIMARY KEY)",
        "DROP TABLE test3"
    );

    // create a different third migration
    let migration3_alt = create_migration!(
        Migration3Alt,
        "migration3_alt",
        "CREATE TABLE test3_alt (id INT PRIMARY KEY)",
        "DROP TABLE test3_alt"
    );

    let mut env = make_test_harness().await?;

    // add the three initial migrations
    env.migrator.add_migration(migration1());
    env.migrator.add_migration(migration2());
    env.migrator.add_migration(migration3());
    env.migrator.apply_all().await?;

    // revert the third migration
    env.migrator.revert_to_inclusive("migration3").await?;

    env.migrator.remove_migration("migration3");

    // add the different third migration and apply
    env.migrator.add_migration(migration3_alt());
    env.migrator.apply_to_inclusive("migration3_alt").await?;

    // Check that test3 table does not exist and test3_alt does
    let mut conn = env.pool.acquire().await?;

    // try to insert data into test3, this should fail
    let res: Result<_, sqlx::Error> = sqlx::query("INSERT INTO test3 VALUES (1)")
        .execute(conn.as_mut())
        .await;
    assert!(matches!(
        res,
        Err(sqlx::Error::Database(_))
            if res
                .unwrap_err()
                .to_string()
                .contains("relation \"test3\" does not exist")
    ));

    // try to insert data into test3_alt, this should succeed
    let res: Result<_, sqlx::Error> = sqlx::query("INSERT INTO test3_alt VALUES (1)")
        .execute(conn.as_mut())
        .await;
    assert!(res.is_ok());

    Ok(())
}

#[tokio::test]
pub async fn test_reordering_migrations() -> Result<(), Box<dyn Error>> {
    let migration1 = create_migration!(
        Migration1,
        "migration1",
        "CREATE TABLE test1 (id INT PRIMARY KEY)",
        "DROP TABLE test1"
    );
    let migration2 = create_migration!(
        Migration2,
        "migration2",
        "CREATE TABLE test2 (id INT PRIMARY KEY)",
        "DROP TABLE test2"
    );
    let migration3 = create_migration!(
        Migration3,
        "migration3",
        "CREATE TABLE test3 (id INT PRIMARY KEY)",
        "DROP TABLE test3"
    );

    let mut env = make_test_harness().await?;

    env.migrator.add_migration(migration1());
    env.migrator.add_migration(migration2());
    env.migrator.add_migration(migration3());

    env.migrator.apply_all().await?;

    env.migrator.remove_all_migrations();
    env.migrator.add_migration(migration1());
    env.migrator.add_migration(migration3());
    env.migrator.add_migration(migration2());

    let res = env.migrator.apply_all().await;
    assert!(matches!(
        res,
        Err(crate::error::Error::HistoryMigrationMismatch { .. })
    ));

    if let Err(crate::error::Error::HistoryMigrationMismatch {
        remote_name,
        local_name,
    }) = res
    {
        assert_eq!(remote_name, "migration2");
        assert_eq!(local_name, "migration3");
    }

    Ok(())
}
