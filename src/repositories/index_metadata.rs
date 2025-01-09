use eyre::{anyhow, Report, Result};
use serde::Deserialize;
use sqlx::Postgres;
use std::sync::Arc;
use tracing::error;

use crate::db::DbConnection;

#[derive(Debug, Deserialize, sqlx::FromRow)]
#[allow(dead_code)]
pub struct IndexMetadata {
    pub id: i64,
    pub current_latest_block_number: i64,
    pub indexing_starting_block_number: i64,
    pub is_backfilling: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub backfilling_block_number: Option<i64>,
}

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
pub async fn get_index_metadata(db: Arc<DbConnection>) -> Result<Option<IndexMetadata>> {
    let db = db.as_ref();
    let result: Result<IndexMetadata, sqlx::Error> = sqlx::query_as(
        r#"
            SELECT 
                id,
                current_latest_block_number,
                indexing_starting_block_number,
                is_backfilling,
                updated_at,
                backfilling_block_number
            FROM index_metadata
            "#,
    )
    .fetch_one(&db.pool)
    .await;

    let result: Option<IndexMetadata> = match result {
        Ok(result) => Some(result),
        Err(err) => match err {
            sqlx::Error::RowNotFound => None,
            err => {
                error!("Failed to get indexer metadata: {}", err);
                return Err(Report::new(err));
            }
        },
    };

    Ok(result)
}

#[allow(dead_code)]
pub async fn set_is_backfilling(db: Arc<DbConnection>, is_backfilling: bool) -> Result<()> {
    let db = db.as_ref();
    let result = sqlx::query(
        r#"
            UPDATE index_metadata
            SET is_backfilling = $1,
            updated_at = CURRENT_TIMESTAMP
            "#,
    )
    .bind(is_backfilling)
    .execute(&db.pool)
    .await?;

    if result.rows_affected() != 1 {
        error!(
            "Failed to set is_backfilling, affecting {} rows",
            result.rows_affected()
        );
        return Err(anyhow!("Failed to set is_backfilling"));
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn set_initial_indexing_status(
    db: Arc<DbConnection>,
    current_latest_block_number: i64,
    indexing_starting_block_number: i64,
    is_backfilling: bool,
) -> Result<()> {
    // Check if there's already an entry, if it does then we can skip and only update.
    let result = sqlx::query(
        r#"
            SELECT id
            FROM index_metadata
            "#,
    )
    .fetch_one(&db.pool)
    .await;

    if result.is_ok() {
        let result = sqlx::query(
            r#"
                UPDATE index_metadata
                SET current_latest_block_number = $1,
                    indexing_starting_block_number = $2,
                    is_backfilling = $3,
                    updated_at = CURRENT_TIMESTAMP
                "#,
        )
        .bind(current_latest_block_number)
        .bind(indexing_starting_block_number)
        .bind(is_backfilling)
        .execute(&db.pool)
        .await?;

        if result.rows_affected() != 1 {
            error!("Failed to update initial indexing status");
            return Err(anyhow!(
                "Failed to update initial indexing status".to_owned(),
            ));
        }

        return Ok(());
    }

    let result = sqlx::query(
        r#"
            INSERT INTO index_metadata (
                current_latest_block_number,
                indexing_starting_block_number,
                is_backfilling
            ) VALUES (
                $1,
                $2,
                $3
            )
            "#,
    )
    .bind(current_latest_block_number)
    .bind(indexing_starting_block_number)
    .bind(is_backfilling)
    .execute(&db.pool)
    .await?;

    if result.rows_affected() != 1 {
        error!("Failed to insert initial indexing status");
        return Err(anyhow!(
            "Failed to insert initial indexing status".to_owned(),
        ));
    }

    Ok(())
}

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
pub async fn update_latest_quick_index_block_number_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    block_number: i64,
) -> Result<()> {
    sqlx::query(
        r#"
            UPDATE index_metadata
            SET current_latest_block_number = $1,
            updated_at = CURRENT_TIMESTAMP
            "#,
    )
    .bind(block_number)
    .execute(&mut **db_tx)
    .await?;

    Ok(())
}

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
pub async fn update_backfilling_block_number_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    block_number: i64,
) -> Result<()> {
    sqlx::query(
        r#"
            UPDATE index_metadata
            SET backfilling_block_number = $1,
            updated_at = CURRENT_TIMESTAMP
            "#,
    )
    .bind(block_number)
    .execute(&mut **db_tx)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // TODO: add tests here with db
}
