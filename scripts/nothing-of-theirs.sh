#!/usr/bin/env bash
# What must never be in a public repository, checked rather than remembered:
# an address belonging to a person, a hostname somebody is served on, a key.
#
# Given a base ref it reads only what a change adds, which is the form that has
# any signal: the tree already holds invented addresses in tests, and a check
# that cries wolf about those is a check somebody turns off. Without one it
# reads the whole tree, which is what a person runs by hand.
set -euo pipefail

cd "$(dirname "$0")/.."

base="${1:-}"

# The reserved names, at any depth, are for invented things.
reserved='(([a-z0-9-]+\.)*example\.(com|org|net)|[a-z0-9-]+\.(test|invalid|localhost|local))'
# Where a placeholder connection string points.
placeholder='host|your-host|localhost|127\.0\.0\.1|postgres|db|\$\{|\{'

patterns=(
  "[a-z0-9._%+-]+@(?!${reserved})[a-z0-9-]+\.[a-z]{2,}"
  '\b[a-z0-9-]+\.(vucod\.com|com\.tr)\b'
  'AKIA[0-9A-Z]{16}'
  'gh[pousr]_[A-Za-z0-9]{36}'
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
  "postgres(ql)?://[^:[:space:]]+:[^@[:space:]]+@(?!${placeholder})"
)

found=0

for pattern in "${patterns[@]}"; do
  if [ -n "$base" ]; then
    matches=$(git diff -U0 "$base...HEAD" -- ':!*.lock' ':!scripts/nothing-of-theirs.sh' \
      | grep -P '^\+' | grep -vP '^\+\+\+' | grep -P -e "$pattern" || true)
  else
    matches=$(git grep -InP -e "$pattern" -- \
      ':!*.lock' ':!scripts/nothing-of-theirs.sh' || true)
  fi

  if [ -n "$matches" ]; then
    echo "$matches"
    found=1
  fi
done

if [ "$found" -eq 1 ]; then
  echo
  echo "The lines above look like they belong to whoever is running this," >&2
  echo "and this repository is public. Take them out of the working tree, and" >&2
  echo "if they are already in a commit, rewrite the history that carries it." >&2
  exit 1
fi

echo "Nothing of anybody's found."
