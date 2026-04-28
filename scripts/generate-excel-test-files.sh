#!/bin/bash
set -e

docker compose exec web npx tsx src/lib/excel/generate-fixtures.ts
