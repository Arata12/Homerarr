use homarr_common::{create_id, generate_secure_random_token, hash_token, HomarrResult};
use sqlx::Row;

use crate::models::InviteRecord;
use crate::pool::map_sqlx_err;
use crate::repos::{naive_to_secs, secs_to_naive};
use crate::Database;

pub async fn create(
    db: &Database,
    creator_id: &str,
    expiration_secs: i64,
) -> HomarrResult<InviteRecord> {
    let id = create_id();
    let token = generate_secure_random_token(24);
    let token_hash = hash_token(&token);
    match db {
        Database::Sqlite(pool) => {
            sqlx::query(
                r#"INSERT INTO "invite" ("id", "token", "expiration_date", "creator_id") VALUES (?, ?, ?, ?)"#,
            )
            .bind(&id)
            .bind(&token_hash)
            .bind(expiration_secs)
            .bind(creator_id)
            .execute(pool)
            .await
            .map_err(map_sqlx_err)?;
        }
        Database::Postgres(pool) => {
            sqlx::query(
                r#"INSERT INTO "invite" ("id", "token", "expiration_date", "creator_id") VALUES ($1, $2, $3, $4)"#,
            )
            .bind(&id)
            .bind(&token_hash)
            .bind(secs_to_naive(expiration_secs))
            .bind(creator_id)
            .execute(pool)
            .await
            .map_err(map_sqlx_err)?;
        }
    }
    Ok(InviteRecord {
        id,
        token,
        expiration_date: expiration_secs,
        creator_id: creator_id.to_string(),
    })
}

pub async fn list_all(db: &Database) -> HomarrResult<Vec<InviteRecord>> {
    match db {
        Database::Sqlite(pool) => {
            let rows = sqlx::query(r#"SELECT * FROM "invite" ORDER BY "expiration_date" DESC"#)
                .fetch_all(pool)
                .await
                .map_err(map_sqlx_err)?;
            rows.iter()
                .map(|r| {
                    Ok(InviteRecord {
                        id: r.try_get("id")?,
                        token: String::new(),
                        expiration_date: r.try_get("expiration_date")?,
                        creator_id: r.try_get("creator_id")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlx_err)
        }
        Database::Postgres(pool) => {
            let rows = sqlx::query(r#"SELECT * FROM "invite" ORDER BY "expiration_date" DESC"#)
                .fetch_all(pool)
                .await
                .map_err(map_sqlx_err)?;
            rows.iter()
                .map(|r| {
                    let dt: chrono::NaiveDateTime = r.try_get("expiration_date")?;
                    Ok(InviteRecord {
                        id: r.try_get("id")?,
                        token: String::new(),
                        expiration_date: naive_to_secs(dt),
                        creator_id: r.try_get("creator_id")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlx_err)
        }
    }
}

pub async fn find_by_token(db: &Database, token: &str) -> HomarrResult<Option<InviteRecord>> {
    let token_hash = hash_token(token);
    match db {
        Database::Sqlite(pool) => {
            let row = sqlx::query(r#"SELECT * FROM "invite" WHERE "token" = ?"#)
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .map_err(map_sqlx_err)?;
            row.map(|r| {
                Ok(InviteRecord {
                    id: r.try_get("id")?,
                    token: token.to_string(),
                    expiration_date: r.try_get("expiration_date")?,
                    creator_id: r.try_get("creator_id")?,
                })
            })
            .transpose()
            .map_err(map_sqlx_err)
        }
        Database::Postgres(pool) => {
            let row = sqlx::query(r#"SELECT * FROM "invite" WHERE "token" = $1"#)
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .map_err(map_sqlx_err)?;
            row.map(|r| {
                let dt: chrono::NaiveDateTime = r.try_get("expiration_date")?;
                Ok(InviteRecord {
                    id: r.try_get("id")?,
                    token: token.to_string(),
                    expiration_date: naive_to_secs(dt),
                    creator_id: r.try_get("creator_id")?,
                })
            })
            .transpose()
            .map_err(map_sqlx_err)
        }
    }
}

pub async fn delete(db: &Database, id: &str) -> HomarrResult<bool> {
    let affected = match db {
        Database::Sqlite(pool) => sqlx::query(r#"DELETE FROM "invite" WHERE "id" = ?"#)
            .bind(id)
            .execute(pool)
            .await
            .map_err(map_sqlx_err)?
            .rows_affected(),
        Database::Postgres(pool) => sqlx::query(r#"DELETE FROM "invite" WHERE "id" = $1"#)
            .bind(id)
            .execute(pool)
            .await
            .map_err(map_sqlx_err)?
            .rows_affected(),
    };
    Ok(affected > 0)
}

/// Atomically consumes an unexpired invite by token; true only for the single caller that deleted it.
pub async fn consume_by_token(db: &Database, token: &str, now_secs: i64) -> HomarrResult<bool> {
    let token = hash_token(token);
    let affected = match db {
        Database::Sqlite(pool) => {
            sqlx::query(r#"DELETE FROM "invite" WHERE "token" = ? AND "expiration_date" > ?"#)
                .bind(&token)
                .bind(now_secs)
                .execute(pool)
                .await
                .map_err(map_sqlx_err)?
                .rows_affected()
        }
        Database::Postgres(pool) => {
            sqlx::query(r#"DELETE FROM "invite" WHERE "token" = $1 AND "expiration_date" > $2"#)
                .bind(&token)
                .bind(secs_to_naive(now_secs))
                .execute(pool)
                .await
                .map_err(map_sqlx_err)?
                .rows_affected()
        }
    };
    Ok(affected == 1)
}
