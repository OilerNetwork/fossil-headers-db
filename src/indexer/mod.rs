pub mod batch_service;
pub mod quick_service;

// TODO
// [x] 1. start a 'job' and add the job info to the job table, including statuses, etc
// [x] 2. spin up multiple of that job
// [x] 3. record indexing information to a metadata table, including current latest indexed etc.
// [x] 4. quick indexing should always be at the head, but need to use metadata to keep track which is the latest indexed and continue indexing from there
// [x] 5. batch indexing can do by batch, but should be backfilling in reverse blocks to allow for latest blocks to work whilst the batch is indexing
// 6. batch indexing in addition need to handle after migration back filling, design the mechanism for triggering this.
// 7. batch indexing can also additionally have a repair mode to check if there's something missing, but that should ideally never happen. (?)
