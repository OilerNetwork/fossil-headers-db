# repositories

This module contains code that interacts with the database.

## Testing

Testing this module requires setting up a local database. The database and the environment can be setup via the compose file provided at the root of the project. You can start the services like so:

```bash
docker compose -f docker-compose.test.yml up
```

Make sure to also run the migration before running the tests:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5433/fossil_test ./run-migrations.sh
```

After the service is spun up, you can now run the test in the module:

```bash
cargo test
```
