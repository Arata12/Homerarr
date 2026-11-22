//! Migration of databases created by upstream Homarr v1/v2 (TypeScript, Drizzle).
//!
//! The v3 SQLite schema is a superset of upstream's (same table and column names), so data is
//! copied table by table into a freshly migrated v3 database using the columns both share.

use std::path::{Path, PathBuf};
use std::time::Duration;

use homarr_common::{HomarrError, HomarrResult};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, ConnectOptions, Connection, Row, SqliteConnection};

use crate::pool::{map_sqlx_err, sqlite_file_path};
use crate::Database;

/// Never copied: Drizzle/sqlx bookkeeping, and token tables (v3 stores token hashes, so
/// upstream sessions, invites and verification tokens cannot be reused).
const SKIPPED_TABLES: &[&str] = &[
    "__drizzle_migrations",
    "_sqlx_migrations",
    "session",
    "invite",
    "verificationToken",
];

#[derive(Debug, Default)]
pub struct LegacyImportSummary {
    /// Copied tables and their row counts.
    pub tables: Vec<(String, u64)>,
    /// Upstream tables that do not exist in v3.
    pub skipped_tables: Vec<String>,
    pub sessions_dropped: i64,
}

fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn remove_sqlite_files(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(with_suffix(path, suffix));
    }
}

async fn column_names(
    conn: &mut SqliteConnection,
    schema: &str,
    table: &str,
) -> HomarrResult<Vec<String>> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info(?, ?) ORDER BY cid")
        .bind(table)
        .bind(schema)
        .fetch_all(&mut *conn)
        .await
        .map_err(map_sqlx_err)
}

/// Copies every table of an upstream Homarr v1/v2 SQLite file into `db`, which must be a migrated,
/// empty v3 SQLite database. Runs in one transaction and checks foreign keys and integrity before
/// committing.
pub async fn import_legacy_sqlite(
    db: &Database,
    legacy_path: &Path,
) -> HomarrResult<LegacyImportSummary> {
    let Database::Sqlite(pool) = db else {
        // shortcut: SQLite targets only; a PostgreSQL target needs a cross-driver row copy.
        return Err(HomarrError::BadRequest(
            "Importing an upstream Homarr database is only supported into a SQLite DB_URL".into(),
        ));
    };
    if !legacy_path.is_file() {
        return Err(HomarrError::NotFound(format!(
            "Legacy database {} not found",
            legacy_path.display()
        )));
    }
    let mut conn = pool.acquire().await.map_err(map_sqlx_err)?;
    let users: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "user""#)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_sqlx_err)?;
    if users > 0 {
        return Err(HomarrError::Conflict(
            "The target database already has users; point DB_URL at a new database before importing"
                .into(),
        ));
    }

    // Must be set outside a transaction; rows are copied in table order, not dependency order.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .map_err(map_sqlx_err)?;
    let result = match sqlx::query("ATTACH DATABASE ? AS legacy")
        .bind(legacy_path.to_string_lossy().into_owned())
        .execute(&mut *conn)
        .await
    {
        Ok(_) => {
            let result = copy_tables(&mut conn).await;
            let _ = sqlx::query("DETACH DATABASE legacy")
                .execute(&mut *conn)
                .await;
            result
        }
        Err(err) => Err(map_sqlx_err(err)),
    };
    let _ = sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await;
    result
}

async fn copy_tables(conn: &mut SqliteConnection) -> HomarrResult<LegacyImportSummary> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM legacy.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(map_sqlx_err)?;
    if !tables.iter().any(|t| t == "__drizzle_migrations") {
        return Err(HomarrError::BadRequest(
            "Not an upstream Homarr v1/v2 database (no __drizzle_migrations table)".into(),
        ));
    }

    let mut summary = LegacyImportSummary::default();
    if tables.iter().any(|t| t == "session") {
        summary.sessions_dropped = sqlx::query_scalar(r#"SELECT COUNT(*) FROM legacy."session""#)
            .fetch_one(&mut *conn)
            .await
            .map_err(map_sqlx_err)?;
    }

    let mut tx = conn.begin().await.map_err(map_sqlx_err)?;
    for table in tables {
        if SKIPPED_TABLES.contains(&table.as_str()) {
            continue;
        }
        let target_cols = column_names(&mut tx, "main", &table).await?;
        let cols: Vec<String> = column_names(&mut tx, "legacy", &table)
            .await?
            .into_iter()
            .filter(|c| target_cols.contains(c))
            .map(|c| quote(&c))
            .collect();
        if cols.is_empty() {
            summary.skipped_tables.push(table);
            continue;
        }
        let list = cols.join(", ");
        let t = quote(&table);
        sqlx::query(AssertSqlSafe(format!("DELETE FROM main.{t}")))
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_err)?;
        let copied = sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO main.{t} ({list}) SELECT {list} FROM legacy.{t}"
        )))
        .execute(&mut *tx)
        .await
        .map_err(|err| HomarrError::Database(format!("Copying table {table} failed: {err}")))?
        .rows_affected();
        summary.tables.push((table, copied));
    }

    let fk_violations = sqlx::query("PRAGMA main.foreign_key_check")
        .fetch_all(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;
    if let Some(row) = fk_violations.first() {
        let table: String = row.try_get(0).unwrap_or_default();
        return Err(HomarrError::Database(format!(
            "Foreign key check failed after copying ({} violation(s), first in table {table})",
            fk_violations.len()
        )));
    }
    let integrity: String = sqlx::query_scalar("PRAGMA main.integrity_check")
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;
    if integrity != "ok" {
        return Err(HomarrError::Database(format!(
            "Integrity check failed after copying: {integrity}"
        )));
    }
    tx.commit().await.map_err(map_sqlx_err)?;
    Ok(summary)
}

/// If the SQLite file behind `url` was created by upstream Homarr v1/v2, replaces it with a v3
/// database holding the same data. The original is first saved as `<db>.v2-backup-<timestamp>`
/// and is left untouched on any failure. Does nothing for other databases.
pub async fn upgrade_legacy_sqlite_if_needed(url: &str) -> HomarrResult<()> {
    let Some(path) = sqlite_file_path(url) else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let mut legacy = SqliteConnectOptions::new()
        .filename(&path)
        .busy_timeout(Duration::from_secs(5))
        .connect()
        .await
        .map_err(map_sqlx_err)?;
    let is_legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '__drizzle_migrations'",
    )
    .fetch_one(&mut legacy)
    .await
    .map_err(map_sqlx_err)?;
    if is_legacy == 0 {
        let _ = legacy.close().await;
        return Ok(());
    }

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let backup = with_suffix(&path, &format!(".v2-backup-{stamp}"));
    let staging = with_suffix(&path, ".v3-migrating");
    tracing::warn!(
        path = %path.display(),
        backup = %backup.display(),
        "Found a database from upstream Homarr v1/v2; migrating it to Homarr v3 (set LEGACY_DB_AUTO_MIGRATE=false to disable)"
    );

    // VACUUM INTO writes a consistent snapshot that includes pages still in the WAL.
    if let Err(err) = sqlx::query("VACUUM INTO ?")
        .bind(backup.to_string_lossy().into_owned())
        .execute(&mut legacy)
        .await
    {
        let _ = legacy.close().await;
        return Err(HomarrError::Database(format!(
            "Could not back up the upstream Homarr database to {}: {err}. Make sure the database directory is writable by the Homarr user (e.g. chown -R 65532:65532 /appdata)",
            backup.display()
        )));
    }

    let summary = match build_v3_copy(&staging, &backup).await {
        Ok(summary) => summary,
        Err(err) => {
            let _ = legacy.close().await;
            remove_sqlite_files(&staging);
            return Err(HomarrError::Database(format!(
                "Migrating the upstream Homarr database failed: {err}. {} was left untouched (backup at {}). Restore the old image, or set LEGACY_DB_AUTO_MIGRATE=false and point DB_URL at a new database; see docs/MIGRATING-FROM-HOMARR.md",
                path.display(),
                backup.display()
            )));
        }
    };

    // Fold the WAL into the main file, then drop it so it is not replayed onto the new database.
    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut legacy)
        .await;
    let _ = legacy.close().await;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(with_suffix(&path, suffix));
    }
    std::fs::rename(&staging, &path).map_err(|err| {
        HomarrError::Database(format!(
            "Could not replace {} with the migrated database {}: {err}",
            path.display(),
            staging.display()
        ))
    })?;

    for (table, rows) in &summary.tables {
        tracing::info!(table = %table, rows, "Migrated upstream table");
    }
    for table in &summary.skipped_tables {
        tracing::warn!(table = %table, "Skipped upstream table that Homarr v3 does not have");
    }
    tracing::warn!(
        sessions_dropped = summary.sessions_dropped,
        backup = %backup.display(),
        "Upstream Homarr database migrated. Sessions and invites were dropped: everyone must log in again"
    );
    Ok(())
}

async fn build_v3_copy(staging: &Path, legacy: &Path) -> HomarrResult<LegacyImportSummary> {
    remove_sqlite_files(staging);
    // Rollback journal (not WAL) so the finished file is self-contained and can be renamed.
    let opts = SqliteConnectOptions::new()
        .filename(staging)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(map_sqlx_err)?;
    let db = Database::Sqlite(pool.clone());
    let result = match db.migrate().await {
        Ok(()) => import_legacy_sqlite(&db, legacy).await,
        Err(err) => Err(err),
    };
    pool.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use homarr_core::DatabaseDriver;

    async fn count(pool: &sqlx::SqlitePool, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn upgrades_upstream_database_in_place() {
        let dir = std::env::temp_dir().join(format!(
            "homarr-legacy-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("db.sqlite");
        let url = path.to_string_lossy().into_owned();

        // Upstream v2 schema = v3 schema minus the v3-only columns, plus Drizzle's table.
        {
            let db = Database::connect(DatabaseDriver::Sqlite, &url)
                .await
                .unwrap();
            db.migrate().await.unwrap();
            let Database::Sqlite(pool) = &db else {
                unreachable!()
            };
            sqlx::raw_sql(
                r#"
                DROP TABLE "_sqlx_migrations";
                ALTER TABLE "apiKey" DROP COLUMN "read_only";
                ALTER TABLE "custom_widget_v2_definition" DROP COLUMN "workshop";
                ALTER TABLE "integration" DROP COLUMN "external_url";
                ALTER TABLE "integration" DROP COLUMN "restricted";
                CREATE TABLE "__drizzle_migrations" ("id" integer PRIMARY KEY, "hash" text NOT NULL, "created_at" numeric);
                INSERT INTO "__drizzle_migrations" ("hash", "created_at") VALUES ('abc', 1);
                CREATE TABLE "upstream_only" ("id" text);
                INSERT INTO "user" ("id", "name", "password") VALUES ('u1', 'admin', '$2a$10$hash');
                INSERT INTO "board" ("id", "name", "creator_id") VALUES ('b1', 'home', 'u1');
                INSERT INTO "integration" ("id", "name", "url", "kind") VALUES ('i1', 'Sonarr', 'http://sonarr:8989', 'sonarr');
                INSERT INTO "integrationSecret" ("kind", "value", "updated_at", "integration_id") VALUES ('apiKey', 'cipher.iv', 1, 'i1');
                INSERT INTO "session" ("session_token", "user_id", "expires") VALUES ('token', 'u1', 4102444800);
                "#,
            )
            .execute(pool)
            .await
            .unwrap();
            pool.close().await;
        }

        upgrade_legacy_sqlite_if_needed(&url).await.unwrap();

        let db = Database::connect(DatabaseDriver::Sqlite, &url)
            .await
            .unwrap();
        db.migrate().await.unwrap();
        let Database::Sqlite(pool) = &db else {
            unreachable!()
        };
        let user = crate::repos::users::find_by_id(&db, "u1")
            .await
            .unwrap()
            .expect("user copied");
        assert_eq!(user.name.as_deref(), Some("admin"));
        assert_eq!(
            count(pool, r#"SELECT COUNT(*) FROM "board" WHERE "id" = 'b1' AND "creator_id" = 'u1'"#).await,
            1
        );
        assert_eq!(
            count(pool, r#"SELECT COUNT(*) FROM "integration" WHERE "id" = 'i1' AND "restricted" = 0"#).await,
            1
        );
        assert_eq!(
            count(pool, r#"SELECT COUNT(*) FROM "integrationSecret" WHERE "value" = 'cipher.iv'"#).await,
            1
        );
        assert_eq!(count(pool, r#"SELECT COUNT(*) FROM "session""#).await, 0);
        assert_eq!(
            count(pool, "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('__drizzle_migrations', 'upstream_only')").await,
            0
        );
        pool.close().await;

        let backups = || {
            std::fs::read_dir(&dir)
                .unwrap()
                .filter(|e| {
                    e.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("db.sqlite.v2-backup-")
                })
                .count()
        };
        assert_eq!(backups(), 1);
        assert!(!dir.join("db.sqlite.v3-migrating").exists());

        // Already migrated: nothing happens the second time.
        upgrade_legacy_sqlite_if_needed(&url).await.unwrap();
        assert_eq!(backups(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
