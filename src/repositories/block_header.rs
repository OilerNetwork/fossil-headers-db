use serde::Deserialize;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use crate::rpc::BlockHeaderWithFullTransaction;

use super::repository::RepositoryError;

pub trait BlockHeaderaRepositoryTrait {
    async fn new(pool: Arc<Pool<Postgres>>) -> Self;
    async fn write_blockheader(
        &self,
        block_header: BlockHeaderWithFullTransaction,
    ) -> Result<(), RepositoryError>;
}

// Model is used to interact with the database
pub struct BlockHeaderRepository(Arc<Pool<Postgres>>);

// impl BlockHeaderaRepositoryTrait for BlockHeaderRepository {
//     fn new(pool: Arc<Pool<Postgres>>) -> Self {
//         IndexMetadataRepository(pool)
//     }

//     async fn get_index_metadata(&self) -> Result<Option<IndexMetadata>, RepositoryError> {
//         let result = sqlx::query_as(
//             r#"
//             SELECT id, current_latest_block_number, indexing_starting_block_number, is_backfilling, updated_at
//             FROM indexer_metadata
//             "#,
//         )
//         .fetch_one(&*self.0)
//         .await;

//         let result = match result {
//             Ok(result) => Some(result),
//             Err(err) => match err {
//                 sqlx::Error::RowNotFound => None,
//                 _ => return Err(RepositoryError::DatabaseError(err)),
//             },
//         };

//         Ok(result)
//     }

//     async fn set_is_backfilling(&self, is_backfilling: bool) -> Result<(), RepositoryError> {
//         let result = sqlx::query(
//             r#"
//             UPDATE indexer_metadata
//             SET is_backfilling = $1,
//             updated_at = CURRENT_TIMESTAMP
//             "#,
//         )
//         .bind(is_backfilling)
//         .execute(&*self.0)
//         .await?;

//         if result.rows_affected() != 1 {
//             error!(
//                 "Failed to set is_backfilling, affecting {} rows",
//                 result.rows_affected()
//             );
//             return Err(RepositoryError::UpdateError(
//                 "Failed to set is_backfilling".to_owned(),
//             ));
//         }

//         Ok(())
//     }

//     async fn set_initial_indexing_status(
//         &self,
//         current_latest_block_number: i64,
//         indexing_starting_block_number: i64,
//         is_backfilling: bool,
//     ) -> Result<(), RepositoryError> {
//         // Check if there's already an entry, if it does then we can skip and only update.
//         let result = sqlx::query(
//             r#"
//             SELECT id
//             FROM indexer_metadata
//             "#,
//         )
//         .fetch_one(&*self.0)
//         .await;

//         if result.is_ok() {
//             let result = sqlx::query(
//                 r#"
//                 UPDATE indexer_metadata
//                 SET current_latest_block_number = $1,
//                     indexing_starting_block_number = $2,
//                     is_backfilling = $3
//                     updated_at = CURRENT_TIMESTAMP
//                 "#,
//             )
//             .bind(current_latest_block_number)
//             .bind(indexing_starting_block_number)
//             .bind(is_backfilling)
//             .execute(&*self.0)
//             .await?;

//             if result.rows_affected() != 1 {
//                 error!("Failed to update initial indexing status");
//                 return Err(RepositoryError::UpdateError(
//                     "Failed to update initial indexing status".to_owned(),
//                 ));
//             }

//             return Ok(());
//         }

//         let result = sqlx::query(
//             r#"
//             INSERT INTO indexer_metadata (
//                 current_latest_block_number,
//                 indexing_starting_block_number,
//                 is_backfilling
//             ) VALUES (
//                 $1,
//                 $2,
//                 $3
//             )
//             "#,
//         )
//         .bind(current_latest_block_number)
//         .bind(indexing_starting_block_number)
//         .bind(is_backfilling)
//         .execute(&*self.0)
//         .await?;

//         if result.rows_affected() != 1 {
//             error!("Failed to insert initial indexing status");
//             return Err(RepositoryError::UpdateError(
//                 "Failed to insert initial indexing status".to_owned(),
//             ));
//         }

//         Ok(())
//     }

//     async fn update_latest_quick_index_block_number(
//         &self,
//         block_number: i64,
//     ) -> Result<(), RepositoryError> {
//         let result = sqlx::query(
//             r#"
//             UPDATE indexer_metadata
//             SET current_latest_block_number = $1,
//             updated_at = CURRENT_TIMESTAMP
//             "#,
//         )
//         .bind(block_number)
//         .execute(&*self.0)
//         .await?;

//         if result.rows_affected() != 1 {
//             error!(
//                 "Failed to update latest quick index block number, affecting {} rows",
//                 result.rows_affected()
//             );
//             return Err(RepositoryError::UpdateError(
//                 "Failed to update latest quick index block number".to_owned(),
//             ));
//         }

//         Ok(())
//     }
// }

#[cfg(test)]
mod tests {
    // TODO: add tests here with db
}
