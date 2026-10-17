#!/bin/bash
# Tear down the local docker-compose stack.
#
# Default: stop containers, preserve the postgres volume. Re-running
# ./scripts/up.sh afterwards brings everything back with data intact.
#
# Flags:
#   --volumes   Also remove the postgres volume (destroys all local
#               data). Equivalent to `./scripts/up.sh --fresh` on
#               the next bring-up.
#   -h | --help Show this help.
#
# Examples:
#   ./scripts/down.sh            # stop, keep data
#   ./scripts/down.sh --volumes  # stop and wipe data

set -euo pipefail

VOLUMES=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --volumes) VOLUMES=true; shift ;;
    -h|--help)
      sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown flag: $1" >&2
      echo "Usage: $0 [--volumes]" >&2
      exit 1
      ;;
  esac
done

if [ "$VOLUMES" = true ]; then
  echo "⚠️  --volumes: stopping stack and removing postgres volume..."
  docker compose down -v
else
  docker compose down
fi

echo "Stack down."
