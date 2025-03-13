# Fossil Indexer

Indexer for Blockheaders DB.

## Table of contents

- [Fossil Indexer](#fossil-indexer)
  - [Table of contents](#table-of-contents)
  - [Prerequisites](#prerequisites)
    - [Additional prerequisites](#additional-prerequisites)
  - [Step by step guide](#step-by-step-guide)
    - [1. Installation](#1-installation)
    - [2. Configure your application](#2-configure-your-application)
    - [3. Build and run the application](#3-build-and-run-the-application)
    - [4. Querying the database](#4-querying-the-database)
    - [5. Stopping the indexer](#5-stopping-the-indexer)
  - [Development](#development)
    - [Running the application binary](#running-the-application-binary)
    - [Running tests](#running-tests)
    - [Configuration](#configuration)
      - [1. `INDEX_TRANSACTION`](#1-index_transaction)

## Prerequisites

Ensure you have the following installed before starting:

- Rust: To build the indexer binaries. You will need rust in order to run the indexer. You can visit https://www.rust-lang.org/tools/install in order to get more information on how to install rust on your machine.
- Docker: To run and build images for the indexers. Visit https://docs.docker.com/get-docker/ to get more details as to how you may be able to install docker on your machine.
- Git: For version control and downloading code. Go to https://git-scm.com/downloads to get instructions on how to install git for your machine.

### Additional prerequisites

As this is an indexer and it relies on calling the RPC endpoint of a blockchain node in order to get data on the blockchain, you might want to create an account with a RPC provider such provider is [Infura](https://www.infura.io/).

## Step by step guide

This step by step guide utilizes docker in order to simplify the setup process for most of its components. If you are working on this project and would like to run it without docker, or want more details regarding how to configure the indexer, please refer to the [Development](#development) section.

> Remember to install all the prerequisites before starting, or something might fail along the way.

### 1. Installation

1. Open your terminal (Command prompt for windows, Terminal on Max/Linux)
2. Clone the project down by running the following commands:

```sh
git clone https://github.com/OilerNetwork/fossil-headers-db.git
```

### 2. Configure your application

Now we can configure your application via editing the `.env` file.

1. Copy the contents of `.env.example` to a new file called `.env` via the following:

```sh
cp .env.example .env
```

2. Fill in the values for `NODE_CONNECTION_STRING` which is the URL for the RPC endpoint of a full Ethereum node. You may get such a url from RPC providers like [Infura](https://www.infura.io/), which usually have RPC URL in the form of `https://sepolia.infura.io/v3/<infura_api_key>`.

3. Remember to save the file after adding the values. The file should similar to this after your change:

```sh
NODE_CONNECTION_STRING=https://sepolia.infura.io/v3/<infura_api_key> # if you use infura
ROUTER_ENDPOINT=0.0.0.0:5050
INDEX_TRANSACTIONS=false # flag to index transactions
```

### 3. Build and run the application

1. Start running the application via docker compose. Depending on the version of your docker the command might slightly differ:

```sh
docker compose -f docker-compose.local.yml up -d --build
```

> if you are using an older version of docker then it might be `docker-compose` instead of `docker compose`

This spins up both the indexer and a postgres database for the indexer.

2. The building process might take a while, but once its done you should see something like this:

```sh
[+] Running 2/2
 ✔ Container fossil-headers-db-db-1       Healthy                                                                                                                                                                         0.5s
 ✔ Container fossil-headers-db-indexer-1  Running
```

3. You may run the following command in order to check the status of the indexer:

```sh
docker compose -f docker-compose.local.yml ps
```

which should show you the status like so:

```sh
NAME                          IMAGE                       COMMAND                  SERVICE   CREATED          STATUS                    PORTS
fossil-headers-db-db-1        postgres:16-alpine          "docker-entrypoint.s…"   db        27 minutes ago   Up 27 minutes (healthy)   0.0.0.0:5432->5432/tcp
fossil-headers-db-indexer-1   fossil-headers-db-indexer   "/usr/app/fossil_ind…"   indexer   27 minutes ago   Up 26 minutes             0.0.0.0:5050->5050/tcp
```

4. You can also look at the current logs to see how the indexing is proceeding.

```sh
docker compose -f docker-compose.local.yml logs -f -t
```

### 4. Querying the database 

With the indexer running, now you can query the database to get the required information. The database should be available at `postgres://postgres:postgres@localhost:5432/postgres`, and you can use any tool to connect to the database to access or query it. Here we provide some simple examples of querying the database through the postgresql container.

1. First, get the container id of the database via docker.

```sh
docker ps
```

You should see something like this:

```sh
CONTAINER ID   IMAGE                       COMMAND                  CREATED         STATUS                   PORTS                    NAMES
5d7bf5e20a52   fossil-headers-db-indexer   "/usr/app/fossil_ind…"   2 minutes ago   Up 2 minutes             0.0.0.0:5050->5050/tcp   fossil-headers-db-indexer-1
e16f4ab0016f   postgres:16-alpine          "docker-entrypoint.s…"   2 minutes ago   Up 2 minutes (healthy)   0.0.0.0:5432->5432/tcp   fossil-headers-db-db-1
```

The information you are interested in is the container ID of the container named `fossil-headers-db-db-1`, or something similar. In this case that ID is `e16f4ab0016f`.

2. You can now query the block headers information. Here we query the gas information for the latest 5 blocks from the database.

```sh
docker exec e16f4ab0016f  psql -U postgres -d postgres -c "SELECT number, gas_limit, gas_used, base_fee_per_gas, blob_gas_used, excess_blob_gas FROM blockheaders ORDER BY number DESC LIMIT 5;"
```

Replace `e16f4ab0016f` with the container id for your database.


3. You can also query the current indexing progress by querying against the `index_metadata` table.

```sh
docker exec e16f4ab0016f psql -U postgres -d postgres -c "SELECT * FROM index_metadata;"
```

Replace `e16f4ab0016f` with the container id for your database.

### 5. Stopping the indexer

You can stop your indexer and your database like so:

```sh
docker compose -f docker-compose.local.yml stop
```

If you however decides to remove the database and **reset all your progress**, you can do the following:


```sh
docker compose -f docker-compose.local.yml down
```


## Development

This section contains details for those who might want to develop further on the project.

### Running the application binary

Before running the application, you should know it relies on a few components:

1. A postgresql database
2. A ethereum node with an available RPC endpoint

The connection url for these components should be provided via the `.env` file. An example is already available at `.env.test`

```sh
DB_CONNECTION_STRING=postgres://postgres:postgres@localhost:5432/postgres
NODE_CONNECTION_STRING=http://x.x.x.x:x
```

You can then run the application by doing the following command:

```sh
cargo run --bin fossil_indexer
```

This should build and run the indexer.

### Running tests

The tests for the indexer requires you to setup an testing environment consisting of a database, which is required to test some of the indexing behavior. Before running the test you would have to run a docker compose file to setup the environment.

```sh
docker compose -f docker-compose.test.yml up
```

After that is setup, you can now run the test via the script provided at [scripts/run-test.sh](scripts/run-test.sh)

```sh
scripts/run-test.sh
```

> If you are facing issue running this on Mac/Linux, it might be a permissions issue.Remember to give it execution permissions via `chmod +x scripts/run-test.sh`.

The scripts just runs `cargo test` with the `DATABASE_URL` field provided, so you can also run it with a different `DATABASE_URL` if so desired.

### Configuration

There are currently only **1** configuration available via environmental variables, and the rest are available only by changing the code itself.

#### 1. `INDEX_TRANSACTION`

Available to configure via env vars. Defaults to `false`

This enables or disables indexing transactions for each of the headers.

<p  align="right">(<a  href="#fossil-indexer">back to top</a>)</p>
