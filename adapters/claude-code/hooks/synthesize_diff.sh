#!/usr/bin/env bash
# Synthesize a unified diff from (filePath, oldString, newString) for the
# Claude Code adapter's session recording.
#
# Usage:   synthesize_diff.sh <filePath> <oldString> <newString>
# Prints the synthesized diff on stdout.
#
# Two correctness concerns:
#
# 1. **Hunk-header counts (P1-8).** A literal `@@ -1 +1 @@` is wrong as
#    soon as either side has more than one line — hector's diff parser
#    uses the header's `new_start` to number added lines, so wrong
#    counts produce wrong line numbers downstream. We emit `1,N` form
#    whenever N > 1.
#
# 2. **Injection scrub (P1-9).** OLD and NEW are arbitrary user content.
#    Without escaping, a NEW value containing `\n+++ b/SECRET\n` becomes
#    a real `+++ b/SECRET` header in the synthesized diff, fooling
#    hector's parser into thinking the edit targets a different file.
#    Any line in the user-provided blocks that *looks* like a diff
#    header gets prefixed with a backslash so the parser ignores it.
#
# This helper is deliberately a separate file so it can be unit-tested
# without spinning up the full hook (see tests/synthesize_diff.sh).

set -euo pipefail

FILE="${1:-}"
OLD="${2:-}"
NEW="${3:-}"

if [[ -z "${FILE}" ]]; then
  echo "usage: synthesize_diff.sh <filePath> <oldString> <newString>" >&2
  exit 2
fi

# Line counts. Empty string ⇒ 0 lines; non-empty ⇒ split-by-newline count,
# matching JS `s.split("\n").length`. We count embedded newlines via
# parameter expansion and add 1 — bash-only, no subshell.
count_lines() {
  if [[ -z "$1" ]]; then
    echo 0
    return
  fi
  local nls="${1//[^$'\n']/}"
  echo $(( ${#nls} + 1 ))
}

OLD_LINES=$(count_lines "${OLD}")
NEW_LINES=$(count_lines "${NEW}")

# Header form: `1` if count <= 1, else `1,N`.
hunk_part() {
  if (( $1 <= 1 )); then
    echo "1"
  else
    echo "1,$1"
  fi
}

HUNK_OLD=$(hunk_part "${OLD_LINES}")
HUNK_NEW=$(hunk_part "${NEW_LINES}")

# Prefix each line with `-` (old) or `+` (new), then scrub any line that
# would look like a diff header to hector's parser. We use awk because it
# handles embedded newlines without choking on shell quoting edge cases.
prefix_and_scrub() {
  local prefix="$1"
  local body="$2"
  if [[ -z "${body}" ]]; then
    return 0
  fi
  printf '%s' "${body}" | awk -v p="${prefix}" '
    {
      line = p $0
      if (line ~ /^(---|\+\+\+|@@) /) {
        line = "\\" line
      }
      print line
    }
  '
}

# Emit the synthesized diff. Header for the real file is hardcoded and
# never scrubbed; only the user-controlled blocks pass through scrub.
printf -- '--- a/%s\n' "${FILE}"
printf -- '+++ b/%s\n' "${FILE}"
printf -- '@@ -%s +%s @@\n' "${HUNK_OLD}" "${HUNK_NEW}"
if [[ -n "${OLD}" ]]; then
  prefix_and_scrub "-" "${OLD}"
fi
if [[ -n "${NEW}" ]]; then
  prefix_and_scrub "+" "${NEW}"
fi
