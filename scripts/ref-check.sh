#!/usr/bin/env bash
# Checks the doc references in Rust comments.
#
# A reference is `(<file>.md)` or `(<file>.md <ID>)`, several as `(cache.md C6, security.md SEC-6)`.
# `<file>` is a file in docs/conventions/, or layers.md (.agents/skills/flow-build/layers.md).
# `<ID>` is AC-<n>, SEC-<n>, a decision-log ID such as C9, or `section <n>` for layers.md.
# Fails when a file or an ID does not exist (an ID under `## Deferred` does not count), or when a reference does not follow the format.
set -euo pipefail

cd "$(dirname "$0")/.."

CONVENTIONS=docs/conventions
LAYERS=.agents/skills/flow-build/layers.md

resolve_file() {
  case "$1" in
    layers.md) echo "$LAYERS" ;;
    *) [ -f "$CONVENTIONS/$1" ] && echo "$CONVENTIONS/$1" || true ;;
  esac
}

# Prints one record per finding: `<path>:<line>\t<kind>\t<file>\t<id>`.
# kind is REF (a file, with an ID or empty) or FORMAT (the offending token, then the reason).
scan() {
  git ls-files -z -- 'crates/*.rs' 'xtask/*.rs' | xargs -0 awk '
    function emit(kind, a, b) { printf "%s:%d\t%s\t%s\t%s\n", FILENAME, FNR, kind, a, b }
    /^[[:space:]]*\/\// {
      line = $0
      sub(/^[[:space:]]*\/\/[\/!]?/, "", line)
      # Links and URLs are external; their .md names are not ours.
      gsub(/\[[^]]*\]\(https?:\/\/[^)]*\)/, "", line)
      gsub(/https?:\/\/[^ )>\]]*/, "", line)
      file = ""; open = 0; rest = line
      while (match(rest, /(section [0-9]+|AC-[0-9]+|SEC-[0-9]+|[A-Z][0-9]+|[A-Za-z0-9_\/.-]*[A-Za-z0-9_-]\.md)/)) {
        tok = substr(rest, RSTART, RLENGTH)
        gap = substr(rest, 1, RSTART - 1)
        rest = substr(rest, RSTART + RLENGTH)
        if (tok ~ /\.md$/) {
          if (file != "" && !open) emit("REF", file, "")
          if (tok ~ /\//) { emit("FORMAT", tok, "write the file bare (<file>.md), without a path"); file = ""; continue }
          file = tok; open = 0
          if (rest ~ /^,? +[A-Z][A-Za-z]+[ ,.;)`]/ || rest ~ /^, +[`a-z]/ || rest ~ /^,? +section /) {
            # `.md, Section name`, `.md MVP limits`, `.md, section 6`.
            if (rest !~ /^ +section [0-9]/)
              emit("FORMAT", file, "section name after the file; cite an ID, or say it in words")
          }
        } else if (file != "" && gap ~ /^(,|;| to| and|,? and)? *$/) {
          emit("REF", file, tok); open = 1
        } else {
          if (file != "" && !open) emit("REF", file, "")
          file = ""; open = 0
        }
      }
      if (file != "" && !open) emit("REF", file, "")
    }'
}

declare -A seen=()
broken=0
total=0

fail() { # location, ref, reason
  printf '%s: %s: %s\n' "$1" "$2" "$3"
  broken=$((broken + 1))
}

while IFS=$'\t' read -r loc kind a b; do
  if [ "$kind" = FORMAT ]; then
    fail "$loc" "$a" "$b"
    continue
  fi
  total=$((total + 1))
  path=$(resolve_file "$a")
  if [ -z "$path" ]; then
    fail "$loc" "$a" "unknown file (expected docs/conventions/$a or layers.md)"
    continue
  fi
  [ -z "$b" ] && continue
  key="$a $b"
  if [ -z "${seen[$key]+x}" ]; then
    case "$b" in
      section\ *) grep -qE "^## ${b#section }\\." "$path" && seen[$key]=ok || seen[$key]=bad ;;
      *) # Only the MVP part counts: code never builds what sits under `## Deferred`.
        sed '/^## Deferred/,$d' "$path" | { grep -qF -e "**$b**" -e "| $b |"; } && seen[$key]=ok || seen[$key]=bad ;;
    esac
  fi
  [ "${seen[$key]}" = ok ] || fail "$loc" "$key" "$b is not defined outside ## Deferred in $path"
done < <(scan)

if [ "$broken" -gt 0 ]; then
  echo "ref-check: $broken broken doc reference(s)"
  exit 1
fi
echo "ref-check: ok ($total references)"
