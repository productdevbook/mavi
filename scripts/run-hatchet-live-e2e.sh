#!/usr/bin/env bash
set -Eeuo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_dir=$(cd -- "$script_dir/.." && pwd)
run_suffix=${GITHUB_RUN_ID:-$$}
network_name="mavi-hatchet-live-${run_suffix}"
postgres_name="mavi-hatchet-live-postgres-${run_suffix}"
hatchet_name="mavi-hatchet-live-server-${run_suffix}"
hatchet_image=${HATCHET_IMAGE:-ghcr.io/hatchet-dev/hatchet/hatchet-lite:v0.71.14}
tenant_id=707d0855-80ab-4e1f-a156-f1c4546cbf52
hatchet_password=hatchet-live-password

cleanup() {
  docker rm -f "$hatchet_name" "$postgres_name" >/dev/null 2>&1 || true
  docker network rm "$network_name" >/dev/null 2>&1 || true
}
on_error() {
  docker logs "$hatchet_name" 2>&1 | tail -80 || true
  cleanup
}
trap cleanup EXIT
trap on_error ERR

docker network create "$network_name" >/dev/null
docker run --detach --name "$postgres_name" --network "$network_name" \
  --tmpfs /var/lib/postgresql/data:rw,size=1g \
  --env POSTGRES_USER=hatchet \
  --env POSTGRES_PASSWORD="$hatchet_password" \
  --env POSTGRES_DB=hatchet \
  postgres:15.6 >/dev/null

until docker exec "$postgres_name" pg_isready -U hatchet -d hatchet >/dev/null 2>&1; do
  sleep 1
done

docker run --detach --name "$hatchet_name" --network "$network_name" \
  --publish 127.0.0.1:18888:8888 \
  --publish 127.0.0.1:17077:7077 \
  --env DATABASE_URL="postgres://hatchet:${hatchet_password}@${postgres_name}:5432/hatchet?sslmode=disable" \
  --env SERVER_AUTH_COOKIE_DOMAIN=localhost \
  --env SERVER_AUTH_COOKIE_INSECURE=t \
  --env SERVER_GRPC_BIND_ADDRESS=0.0.0.0 \
  --env SERVER_GRPC_INSECURE=t \
  --env SERVER_GRPC_BROADCAST_ADDRESS=127.0.0.1:17077 \
  --env SERVER_GRPC_PORT=7077 \
  "$hatchet_image" >/dev/null

curl --fail --silent --show-error --retry 90 --retry-delay 2 --retry-all-errors \
  http://127.0.0.1:18888/ >/dev/null

# quickstart creates the default tenant and the command prints only the new
# API token. Keep it in the process environment; it must never be logged.
hatchet_token=$(docker exec "$hatchet_name" ./hatchet-admin token create \
  --config ./config --tenant-id "$tenant_id" --name mavi-ci-live-e2e)
if [[ -z "$hatchet_token" ]]; then
  printf 'Hatchet did not return an API token\n' >&2
  exit 1
fi

cd "$repo_dir/integrations/hatchet-worker"
HATCHET_LIVE_E2E=1 \
MAVI_HATCHET_TOKEN="$hatchet_token" \
MAVI_HATCHET_TENANT_ID="$tenant_id" \
MAVI_HATCHET_GRPC_ADDRESS=127.0.0.1:17077 \
MAVI_HATCHET_SERVER_URL=http://127.0.0.1:18888 \
MAVI_HATCHET_TLS_STRATEGY=none \
MAVI_SITE_ID=00000000-0000-4000-8000-000000000001 \
go test -run '^TestHatchetLiveDispatch$' -count=1 -v
