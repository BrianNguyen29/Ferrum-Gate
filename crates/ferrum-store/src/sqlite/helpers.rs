use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::Result;

pub fn enum_text<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_value(value)?;
    Ok(match json {
        serde_json::Value::String(text) => text,
        other => other.to_string(),
    })
}

pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

pub fn from_json<T: DeserializeOwned>(raw: &str) -> Result<T> {
    Ok(serde_json::from_str(raw)?)
}

pub fn raw_json(row: &SqliteRow) -> Result<String> {
    Ok(row.try_get("raw_json")?)
}

pub async fn fetch_raw_json_by_id(
    pool: &SqlitePool,
    table: &str,
    id_column: &str,
    id_value: &str,
) -> Result<Option<String>> {
    let sql = format!("SELECT raw_json FROM {table} WHERE {id_column} = ?1");
    let row = sqlx::query(&sql)
        .bind(id_value)
        .fetch_optional(pool)
        .await?;
    row.map(|row| raw_json(&row)).transpose()
}

pub async fn fetch_entity_by_id<T: DeserializeOwned>(
    pool: &SqlitePool,
    table: &str,
    id_column: &str,
    id_value: &str,
) -> Result<Option<T>> {
    match fetch_raw_json_by_id(pool, table, id_column, id_value).await? {
        Some(raw) => Ok(Some(from_json(&raw)?)),
        None => Ok(None),
    }
}

pub async fn fetch_entities<T, F>(pool: &SqlitePool, sql: &str, binder: F) -> Result<Vec<T>>
where
    T: DeserializeOwned,
    F: for<'a> FnOnce(
        sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>>,
    )
        -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>>,
{
    let rows = binder(sqlx::query(sql)).fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
        .collect()
}
