#!/bin/bash
set -e

# Rebuilds web, leaves db only if it's down
docker compose up --build -d

echo "Waiting for services to be healthy..."
until docker compose exec web node -e "process.exit(0)" 2>/dev/null; do
  sleep 1
done

echo "Running migrations..."
docker compose exec web npm run migrate

echo "Seeding admin..."
docker compose exec web npm run seed:admin

echo "Dev environment up at http://localhost:5173"
docker compose logs -f web
