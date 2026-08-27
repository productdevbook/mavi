#!/usr/bin/env bash
set -Eeuo pipefail

base_url=${1:?usage: http-p95.sh BASE_URL [SAMPLES] [BASELINE_MS]}
sample_count=${2:-100}
baseline_ms=${3:-100}

if ! [[ "$sample_count" =~ ^[0-9]+$ ]] || (( sample_count < 20 )); then
  printf 'sample count must be an integer greater than or equal to 20\n' >&2
  exit 2
fi
if ! [[ "$baseline_ms" =~ ^[0-9]+$ ]] || (( baseline_ms < 1 )); then
  printf 'baseline must be a positive integer number of milliseconds\n' >&2
  exit 2
fi

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/mavi-http-p95.XXXXXX")
cleanup() {
  rm -f "$tmp_dir/times"
  rmdir "$tmp_dir"
}
trap cleanup EXIT

for ((sample = 0; sample < sample_count; sample++)); do
  curl --fail --silent --show-error --output /dev/null --write-out '%{time_total}\n' \
    "${base_url%/}/healthz" |
    awk '{ printf "%.3f\n", $1 * 1000 }'
done >"$tmp_dir/times"

rank=$(( (sample_count * 95 + 99) / 100 ))
p95_ms=$(sort -n "$tmp_dir/times" | awk -v rank="$rank" 'NR == rank { print; exit }')
budget_ms=$(( (baseline_ms * 110 + 99) / 100 ))

if ! awk -v p95="$p95_ms" -v budget="$budget_ms" 'BEGIN { exit !(p95 <= budget) }'; then
  printf 'HTTP p95 regression: %.3f ms > %d ms budget (baseline %d ms + 10%%)\n' \
    "$p95_ms" "$budget_ms" "$baseline_ms" >&2
  exit 1
fi

printf 'HTTP p95: %.3f ms (budget %d ms, samples %d)\n' \
  "$p95_ms" "$budget_ms" "$sample_count"
