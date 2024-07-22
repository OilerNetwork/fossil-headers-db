<a name="readme-top"></a>

<h3 align="center">Blockheaders/Transactions Tool</h3>

  <p align="center">
    Tool to maintain Blockheaders/Transactions DB. Data is queried via RPC calls.

<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
  </ol>
</details>

<!-- ABOUT THE PROJECT -->

## Getting Started

To get this application runninng locally, follow these steps.

### Prerequisites

What you would need:

- Rust
  ```
  https://www.rust-lang.org/tools/install
  ```

### Installation

1. Create a .env file in the root folder

   _fossil-headers-db/.env_

   ```
    NODE_CONNECTION_STRING=<node_connection_string>
    DB_CONNECTION_STRING=<db_connection_string>
    RUST_LOG=<log_level> [optional]

   ```

1. Clone the repo
   ```sh
   git clone https://github.com/OilerNetwork/fossil-headers-db.git
   ```
1. Build project
   ```sh
   cargo build
   ```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->

## Usage

This application has 2 features that are similar but with different purposes.

### Mode 1 - Update

Fetches blockheaders and transaction data via RPC and writes to DB

**Usage:** _cargo run update_

**Optional parameters:**

1. _start <block_number>_

- First block number to start updating the database from. (Inclusive)
- **Default**: Latest block in the database + 1

1. _end <block_number>_

- Last block number to update the database to. (Inclusive)
- **Default**: Latest block in ethereum chain at the time of running the command

2. _loopsize <num_threads>_

- Max number of threads running at once
- **Default**: Max functional connections for our DB -- 4000

**Examples:**

```sh
cargo run update
```

```sh
cargo run update --loopsize 10
```

```sh
cargo run update --start 19983846
```

```sh
cargo run update --end 19983849
```

```sh
cargo run update --start 19983846 --end 19983849
```

```sh
cargo run update --start 19983846 --end 19983849 --loopsize 10
```

### Mode 2 - Fix

Patches missing blockheaders and transaction data from the DB, retrieving via RPC

**Usage:** _cargo run update_

**Optional parameters:**

1. _start <block_number>_

- First block number to start checking the database from. (Inclusive)
- **Default**: 0

1. _end <block_number>_

- Last block number to check the database to. (Inclusive)
- **Default**: Last entry in the database

**Examples:**

```sh
cargo run fix
```

```sh
cargo run fix --start 19983846
```

```sh
cargo run fix --end 19983849
```

```sh
cargo run fix --start 19983846 --end 19983849
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>
