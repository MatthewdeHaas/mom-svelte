#!/bin/bash
set -e

SEED=false
while getopts "s" opt; do
  case $opt in
    s) SEED=true ;;
    *) echo "Usage: $0 [-s]" >&2; exit 1 ;;
  esac
done

# Rebuilds web, leaves db only if it's down
docker compose up --build -d

echo "Waiting for services to be healthy..."
until docker compose exec web node -e "process.exit(0)" 2>/dev/null; do
  sleep 1
done

echo "Running migrations..."
docker compose exec web pnpm run migrate

echo "Seeding admin..."
docker compose exec web pnpm run seed:admin

if [ "$SEED" = true ]; then
  echo "Seeding dev data..."
  docker compose exec web pnpm run seed:dev
fi

echo "Dev environment up at http://localhost:5173"
docker compose logs -f web
