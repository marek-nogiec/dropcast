#!/bin/sh

set -eu

message_file="${1:-}"

if [ -z "$message_file" ] || [ ! -r "$message_file" ]; then
  printf 'Usage: %s <commit-message-file>\n' "$0" >&2
  exit 2
fi

subject="$(sed -n '1{s/\r$//;p;}' "$message_file")"
pattern='^(build|chore|ci|deps|docs|feat|fix|perf|refactor|revert|style|test)(\([^()[:space:]]+\))?!?: [^[:space:]].*$'

if printf '%s\n' "$subject" | grep -Eq "$pattern"; then
  exit 0
fi

cat >&2 <<'EOF'
The commit subject does not follow Conventional Commits.

Expected: <type>[optional scope][!]: <description>
Example:  feat(cast): add playback queue support
Breaking: refactor(server)!: remove the legacy endpoint

Allowed types: build, chore, ci, deps, docs, feat, fix, perf, refactor,
revert, style, and test.
EOF

exit 1
