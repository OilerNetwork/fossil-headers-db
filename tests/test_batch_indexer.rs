// use eyre::Result;
// use std::sync::{atomic::AtomicBool, Arc};

// use fossil_headers_db::{
//     db::DbConnection,
//     indexer::batch_service::{BatchIndexConfig, BatchIndexer},
// };

// #[tokio::test]
// async fn test_batch_indexer_index_empty_db() -> Result<()> {
//     let config = BatchIndexConfig::default();
//     let db = Arc::new(DbConnection::new("test_connection_string").await?);
//     let should_terminate = Arc::new(AtomicBool::new(false));

//     let indexer = BatchIndexer::new(config, db.clone(), should_terminate.clone()).await?;

//     // Start indexing in a separate task
//     let handle = tokio::spawn(async move { indexer.index().await });

//     // Let it run briefly
//     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//     should_terminate.store(true, std::sync::atomic::Ordering::Relaxed);

//     let result = handle.await?;
//     assert!(result.is_ok());

//     // Verify metadata was created
//     let metadata = get_index_metadata(db).await?;
//     assert!(metadata.is_some());

//     Ok(())
// }

// #[test]
// async fn test_batch_indexer_index_with_existing_data() -> Result<()> {
//     let config = BatchIndexConfig::default();
//     let db = Arc::new(DbConnection::new("test_connection_string").await?);
//     let should_terminate = Arc::new(AtomicBool::new(false));

//     // Set initial block number
//     update_latest_batch_index_block_number_query(db.clone(), 1000).await?;

//     let indexer = BatchIndexer::new(config, db.clone(), should_terminate.clone()).await?;

//     let handle = tokio::spawn(async move { indexer.index().await });

//     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//     should_terminate.store(true, std::sync::atomic::Ordering::Relaxed);

//     let result = handle.await?;
//     assert!(result.is_ok());

//     // Verify block number increased
//     let metadata = get_index_metadata(db).await?;
//     assert!(metadata.is_some());
//     assert!(metadata.unwrap().current_latest_block_number > 1000);

//     Ok(())
// }

// #[tokio::test]
// async fn test_batch_indexer_index_with_interruption() -> Result<()> {
//     let config = BatchIndexConfig::default();
//     let db = Arc::new(DbConnection::new("test_connection_string").await?);
//     let should_terminate = Arc::new(AtomicBool::new(false));

//     let indexer = BatchIndexer::new(config, db.clone(), should_terminate.clone()).await?;

//     // Start and stop multiple times
//     for _ in 0..3 {
//         let indexer_clone = indexer.clone();
//         let handle = tokio::spawn(async move { indexer_clone.index().await });

//         tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
//         should_terminate.store(true, std::sync::atomic::Ordering::Relaxed);

//         let result = handle.await?;
//         assert!(result.is_ok());

//         // Reset termination flag
//         should_terminate.store(false, std::sync::atomic::Ordering::Relaxed);
//     }

//     Ok(())
// }

// #[test]
// async fn test_batch_indexer_index_batch_size() -> Result<()> {
//     let config = BatchIndexConfig {
//         index_batch_size: 5,
//         ..Default::default()
//     };
//     let db = Arc::new(DbConnection::new("test_connection_string").await?);
//     let should_terminate = Arc::new(AtomicBool::new(false));

//     let indexer = BatchIndexer::new(config, db.clone(), should_terminate.clone()).await?;

//     let handle = tokio::spawn(async move { indexer.index().await });

//     tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
//     should_terminate.store(true, std::sync::atomic::Ordering::Relaxed);

//     let result = handle.await?;
//     assert!(result.is_ok());

//     // Verify blocks were indexed in correct batch sizes
//     let metadata = get_index_metadata(db).await?;
//     assert!(metadata.is_some());

//     Ok(())
// }
