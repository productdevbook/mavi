#!/usr/bin/env bash
set -Eeuo pipefail

# Create a self-host backup without putting credentials in the archive.
# The compose project is the boundary: the database dump is made by the
# bundled PostgreSQL container and the files archive by a temporary container
# with the same /data volume as the API.

usage() {
  printf 'Usage: %s [backup-directory]\n' "$0" >&2
  printf '\nThe directory must be new or empty. DATABASE_URL is not printed.\n' >&2
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if (( $# > 1 )); then
  usage
  exit 2
fi

compose_file=${COMPOSE_FILE:-docker-compose.yml}
env_file=${ENV_FILE:-.env}
backup_dir=${1:-"./backups/mavi-$(date -u +%Y%m%dT%H%M%SZ)"}

dotenv_value() {
  local key=$1
  if [[ -n "${!key:-}" ]]; then
    printf '%s' "${!key}"
    return
  fi
  if [[ -f "$env_file" ]]; then
    awk -F= -v key="$key" '$1 == key {sub(/^[^=]*=/, ""); print; exit}' "$env_file"
  fi
}

site_id=$(dotenv_value MAVI_SITE_ID)
mavi_version=$(dotenv_value MAVI_VERSION)
mavi_version=${mavi_version:-unknown}

if [[ ! "$site_id" =~ ^[0-9a-fA-F-]{36}$ ]]; then
  printf 'MAVI_SITE_ID is missing or is not a UUID; refusing to create an unlabelled backup.\n' >&2
  exit 1
fi

if [[ -e "$backup_dir" ]]; then
  if [[ ! -d "$backup_dir" ]]; then
    printf 'Backup target exists and is not a directory: %s\n' "$backup_dir" >&2
    exit 1
  fi
  if [[ -n "$(find "$backup_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
    printf 'Backup target is not empty: %s\n' "$backup_dir" >&2
    exit 1
  fi
else
  mkdir -p -m 700 "$backup_dir"
fi

umask 077
mkdir -p "$backup_dir"

if ! docker compose -f "$compose_file" config --quiet; then
  printf 'Compose configuration is invalid; refusing to back up.\n' >&2
  exit 1
fi

api_was_running=0
panel_was_running=0
if docker compose -f "$compose_file" ps --status running --services | grep -qx api; then
  api_was_running=1
fi
if docker compose -f "$compose_file" ps --status running --services | grep -qx panel; then
  panel_was_running=1
fi

restart_previous_services() {
  if (( api_was_running )); then
    docker compose -f "$compose_file" start api >/dev/null || true
  fi
  if (( panel_was_running )); then
    docker compose -f "$compose_file" start panel >/dev/null || true
  fi
}
trap restart_previous_services EXIT

# Freeze application writes so the database snapshot and file namespace are a
# pair. PostgreSQL remains up for pg_dump; the API and panel are restarted only
# if they were running before this command.
docker compose -f "$compose_file" stop api panel >/dev/null

printf 'Creating PostgreSQL dump...\n' >&2
docker compose -f "$compose_file" exec -T postgres \
  sh -ceu 'pg_dump --format=custom --no-owner --no-acl -U "$POSTGRES_USER" -d "$POSTGRES_DB"' \
  > "$backup_dir/database.dump"

printf 'Creating site file archive...\n' >&2
docker compose -f "$compose_file" run --rm --no-deps --entrypoint tar api \
  -C /data -czf - files > "$backup_dir/files.tar.gz"

cat > "$backup_dir/manifest.json" <<EOF
{
  "format": "mavi.self_host_backup",
  "version": 1,
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "site_id": "$site_id",
  "mavi_version": "$mavi_version",
  "database_dump": "database.dump",
  "files_archive": "files.tar.gz"
}
EOF

(cd "$backup_dir" && sha256sum database.dump files.tar.gz manifest.json > SHA256SUMS)
printf 'Backup created at %s\n' "$backup_dir" >&2
