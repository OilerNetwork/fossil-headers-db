#!/bin/bash
set -e

counter=0

echo "Waiting for database to be ready..."
until sqlx database create && sqlx migrate run; do
  if [ $counter -eq 10 ]; then
    >&2 echo "Database is unavailable - max retries exceeded"
    exit 1
  fi
  ((counter++))
  >&2 echo "Database is unavailable - sleeping"
  sleep 1
done

echo "Migrations completed successfully"
