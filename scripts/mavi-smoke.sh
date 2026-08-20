#!/usr/bin/env bash
set -Eeuo pipefail

base_url=${MAVI_SMOKE_URL:-http://localhost}
setup_enabled=${MAVI_SMOKE_SETUP:-1}

curl --fail --silent --show-error --retry 30 --retry-delay 2 --retry-connrefused \
  "${base_url%/}/healthz" >/dev/null
curl --fail --silent --show-error --retry 30 --retry-delay 2 --retry-connrefused \
  "${base_url%/}/readyz" >/dev/null

openapi=$(curl --fail --silent --show-error "${base_url%/}/openapi.json")
if ! printf '%s' "$openapi" | jq -e '.openapi | type == "string"' >/dev/null; then
  printf 'OpenAPI smoke check failed.\n' >&2
  exit 1
fi

panel_status=$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  "${base_url%/}/admin/")
if [[ "$panel_status" != 200 ]]; then
  printf 'Panel smoke check returned HTTP %s.\n' "$panel_status" >&2
  exit 1
fi

if [[ "$setup_enabled" == 1 ]]; then
  setup_password=$(openssl rand -hex 24)
  setup_response=$(curl --fail --silent --show-error -X POST \
    -H 'content-type: application/json' \
    -d "{\"site_name\":\"Release smoke\",\"email\":\"owner@example.com\",\"name\":\"Owner\",\"password\":\"$setup_password\"}" \
    "${base_url%/}/api/v1/setup")
  if ! printf '%s' "$setup_response" | jq -e '.id | type == "string"' >/dev/null; then
    printf 'Setup smoke check returned an unexpected response.\n' >&2
    exit 1
  fi

  second_status=$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    -X POST -H 'content-type: application/json' \
    -d "{\"site_name\":\"Release smoke\",\"email\":\"owner@example.com\",\"name\":\"Owner\",\"password\":\"$setup_password\"}" \
    "${base_url%/}/api/v1/setup")
  if [[ "$second_status" != 409 ]]; then
    printf 'Setup one-time smoke check returned HTTP %s, expected 409.\n' "$second_status" >&2
    exit 1
  fi
fi

printf 'Mavi smoke checks passed at %s\n' "$base_url" >&2
