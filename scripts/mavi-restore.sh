#!/usr/bin/env bash
set -Eeuo pipefail

# Restore a complete self-host backup. This is intentionally confirmation
# gated: pg_restore --clean replaces the target database and the files archive
# replaces /data/files in the API volume.

usage() {
  printf 'Usage: %s --yes BACKUP-DIRECTORY\n' "$0" >&2
  printf '\nThe target PostgreSQL database and /data/files are replaced.\n' >&2
}

if [[ "${1:-}" != "--yes" || -z "${2:-}" || $# != 2 ]]; then
  usage
  exit 2
fi

backup_dir=$2
compose_file=${COMPOSE_FILE:-docker-compose.yml}
env_file=${ENV_FILE:-.env}

if [[ ! -d "$backup_dir" ]]; then
  printf 'Backup directory does not exist: %s\n' "$backup_dir" >&2
  exit 1
fi
for required_file in manifest.json database.dump files.tar.gz SHA256SUMS; do
  if [[ ! -f "$backup_dir/$required_file" ]]; then
    printf 'Backup is incomplete; missing %s.\n' "$required_file" >&2
    exit 1
  fi
done

command -v jq >/dev/null || {
  printf 'jq is required to validate the backup manifest.\n' >&2
  exit 1
}

(cd "$backup_dir" && sha256sum --check SHA256SUMS)
jq -e '.format == "mavi.self_host_backup" and .version == 1 and (.site_id | type == "string")' \
  "$backup_dir/manifest.json" >/dev/null

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

target_site_id=$(dotenv_value MAVI_SITE_ID)
backup_site_id=$(jq -r '.site_id' "$backup_dir/manifest.json")
if [[ -z "$target_site_id" || "$target_site_id" != "$backup_site_id" ]]; then
  printf 'Backup site_id does not match the configured MAVI_SITE_ID; refusing to restore.\n' >&2
  printf 'Use a new .env with the backup site_id when intentionally restoring elsewhere.\n' >&2
  exit 1
fi

if ! docker compose -f "$compose_file" config --quiet; then
  printf 'Compose configuration is invalid; refusing to restore.\n' >&2
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

docker compose -f "$compose_file" stop api panel >/dev/null

printf 'Restoring PostgreSQL database...\n' >&2
docker compose -f "$compose_file" exec -T postgres \
  sh -ceu 'pg_restore --clean --if-exists --no-owner --no-acl -U "$POSTGRES_USER" -d "$POSTGRES_DB"' \
  < "$backup_dir/database.dump"

printf 'Restoring site files...\n' >&2
docker compose -f "$compose_file" run --rm --no-deps --entrypoint sh api -ceu \
  'rm -rf /data/files && mkdir -p /data/files && tar -xzf - -C /data' \
  < "$backup_dir/files.tar.gz"

printf 'Restore completed for site %s. The API will re-run pending migrations on start.\n' "$target_site_id" >&2
