#!/bin/bash

set -e

# Rebuilds web
# Leaves db only if its down
docker compose up --build -d web

echo "Dev environment up at http://localhost:5173"


sleep 5
docker compose exec web npm run migrate 
docker compose logs -f web
