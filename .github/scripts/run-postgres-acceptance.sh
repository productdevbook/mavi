#!/usr/bin/env bash
set -Eeuo pipefail

# The acceptance tests create their own random site IDs, so independent test
# databases are the safe unit of parallelism. Keeping a whole shard on one
# database also preserves the existing migration/RLS behavior of each test
# command while removing the serial wait between unrelated domains.

database_base_url=${TEST_DATABASE_URL_BASE:?TEST_DATABASE_URL_BASE is required}
runner_temp=${RUNNER_TEMP:-/tmp}

run_shard() {
  local shard=$1
  local database="mavi_server_${shard}"
  local log_file="$runner_temp/mavi-postgres-acceptance-${shard}.log"

  {
    export TEST_DATABASE_URL="${database_base_url%/}/${database}"
    printf 'Acceptance shard %s using %s\n' "$shard" "$database"

    case "$shard" in
      1)
        cargo nextest run -p mavi-storage --test postgres --run-ignored all
        cargo nextest run -p mavi-content --test postgres --run-ignored all
        cargo nextest run -p mavi-identity --test postgres --run-ignored all
        cargo nextest run -p mavi-secrets --test postgres --run-ignored all
        cargo nextest run -p mavi-settings --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test identity --run-ignored all
        cargo nextest run -p mavi-http --test credentials --run-ignored all
        cargo nextest run -p mavi-http --test settings --run-ignored all
        cargo nextest run -p mavi-http --test content_types --run-ignored all
        cargo nextest run -p mavi-http --test content_lifecycle --run-ignored all
        ;;
      2)
        cargo nextest run -p mavi-taxonomy --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test taxonomy --run-ignored all
        cargo nextest run -p mavi-audit --test postgres --run-ignored all
        cargo nextest run -p mavi-trash --test postgres --run-ignored all
        cargo nextest run -p mavi-media --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test media --run-ignored all
        cargo nextest run -p mavi-design --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test design --run-ignored all
        cargo nextest run -p mavi-forms --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test forms --run-ignored all
        cargo nextest run -p mavi-http --test feedback --run-ignored all
        ;;
      3)
        cargo nextest run -p mavi-mail --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test mail --run-ignored all
        cargo nextest run -p mavi-shop --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test shop --run-ignored all
        cargo nextest run -p mavi-courses --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test courses --run-ignored all
        cargo nextest run -p mavi-jobs --test postgres --run-ignored all
        cargo nextest run -p mavi-worker --test postgres --run-ignored all
        cargo nextest run -p mavi-worker --test mail --run-ignored all
        cargo nextest run -p mavi-flows --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test automation --run-ignored all
        ;;
      4)
        cargo nextest run -p mavi-boards --test postgres --run-ignored all
        cargo nextest run -p mavi-analytics --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test boards_analytics --run-ignored all
        cargo nextest run -p mavi-portable --test postgres --run-ignored all
        cargo nextest run -p mavi-http --test portable --run-ignored all
        cargo nextest run -p mavi-http --test runtime --run-ignored all
        cargo nextest run -p mavi-http --test write_fence --run-ignored all
        cargo nextest run -p mavi-http --test mcp --run-ignored all
        cargo nextest run -p mavi-http --test audit_trash --run-ignored all
        ;;
      *)
        printf 'Unknown acceptance shard: %s\n' "$shard" >&2
        return 2
        ;;
    esac

    printf 'Acceptance shard %s passed\n' "$shard"
  } >"$log_file" 2>&1
}

pids=()
for shard in 1 2 3 4; do
  run_shard "$shard" &
  pids[$shard]=$!
done

failed=0
for shard in 1 2 3 4; do
  if wait "${pids[$shard]}"; then
    cat "$runner_temp/mavi-postgres-acceptance-${shard}.log"
  else
    failed=1
    printf '\nAcceptance shard %s failed:\n' "$shard" >&2
    cat "$runner_temp/mavi-postgres-acceptance-${shard}.log" >&2
  fi
done

exit "$failed"
