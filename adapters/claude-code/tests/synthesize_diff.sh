#!/usr/bin/env bash
set -euo pipefail

# Unit-level test for the Claude Code adapter's synthesizeDiff helper.
# Regression coverage for:
#   - P1-8: multi-line oldString/newString must produce a correct
#           `@@ -1,N +1,M @@` hunk header (the old hook always emitted
#           `-1 +1`).
#   - P1-9: a newString containing literal `+++ b/SECRET` (or other
#           header-looking lines) must NOT appear as a real diff header
#           in the synthesized output — otherwise hector's diff parser
#           reframes the edit onto the wrong file.
#
# The helper lives at `adapters/claude-code/hooks/synthesize_diff.sh`
# and is sourced by hook.sh. Calling it directly keeps this fast.

ADAPTER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HELPER="${ADAPTER_DIR}/hooks/synthesize_diff.sh"

if [[ ! -x "${HELPER}" ]]; then
  echo "FAIL: synthesize helper not found or not executable at ${HELPER}"
  exit 1
fi

# Test 1: empty old + single-line new → @@ -1 +1 @@ (no counts).
OUT=$("${HELPER}" "foo.ts" "" "x")
if grep -qE '^@@ -1 \+1 @@$' <<<"${OUT}"; then
  echo "PASS: single-line emits @@ -1 +1 @@"
else
  echo "FAIL: expected '@@ -1 +1 @@', got:"
  echo "${OUT}"
  exit 1
fi

# Test 2: multi-line old + multi-line new → correct counts. P1-8.
OUT=$("${HELPER}" "foo.ts" "a
b" "x
y
z")
if grep -qE '^@@ -1,2 \+1,3 @@$' <<<"${OUT}"; then
  echo "PASS: multi-line emits @@ -1,2 +1,3 @@"
else
  echo "FAIL: expected '@@ -1,2 +1,3 @@', got:"
  echo "${OUT}"
  exit 1
fi

# Test 3: multi-line old + single-line new.
OUT=$("${HELPER}" "foo.ts" "a
b
c" "x")
if grep -qE '^@@ -1,3 \+1 @@$' <<<"${OUT}"; then
  echo "PASS: 3-line old, 1-line new emits @@ -1,3 +1 @@"
else
  echo "FAIL: expected '@@ -1,3 +1 @@', got:"
  echo "${OUT}"
  exit 1
fi

# Test 4: empty old (write-style) + multi-line new.
OUT=$("${HELPER}" "foo.ts" "" "x
y")
if grep -qE '^@@ -1 \+1,2 @@$' <<<"${OUT}"; then
  echo "PASS: empty old, 2-line new emits @@ -1 +1,2 @@"
else
  echo "FAIL: expected '@@ -1 +1,2 @@', got:"
  echo "${OUT}"
  exit 1
fi

# Test 5: P1-9 scrub. newString contains embedded diff headers; after
# synthesis, those embedded headers must not survive as real headers.
EVIL='x
--- a/SECRET
+++ b/SECRET
@@ -1 +1 @@
+pwn'
OUT=$("${HELPER}" "foo.ts" "" "${EVIL}")
# The embedded `+++ b/SECRET` must not appear as a real header line
# (i.e. at column 0 followed by EOL).
if grep -qE '^\+\+\+ b/SECRET$' <<<"${OUT}"; then
  echo "FAIL: embedded '+++ b/SECRET' must be scrubbed, got:"
  echo "${OUT}"
  exit 1
fi
if grep -qE '^--- a/SECRET$' <<<"${OUT}"; then
  echo "FAIL: embedded '--- a/SECRET' must be scrubbed, got:"
  echo "${OUT}"
  exit 1
fi
if grep -qE '^@@ -1 \+1 @@$' <<<"${OUT}" && ! grep -qE '^@@ -1 \+1,5 @@$' <<<"${OUT}"; then
  # The only legitimate @@ header for this case is the synthesized one
  # for foo.ts (-1 +1,5 since EVIL has 5 lines). If a bare '@@ -1 +1 @@'
  # also appears at col 0, the scrub missed it.
  if [[ $(grep -cE '^@@ ' <<<"${OUT}") -gt 1 ]]; then
    echo "FAIL: scrub missed an embedded '@@' header, got:"
    echo "${OUT}"
    exit 1
  fi
fi
# Real header for the actual file must still be there.
if ! grep -qE '^--- a/foo\.ts$' <<<"${OUT}"; then
  echo "FAIL: real '--- a/foo.ts' header missing, got:"
  echo "${OUT}"
  exit 1
fi
if ! grep -qE '^\+\+\+ b/foo\.ts$' <<<"${OUT}"; then
  echo "FAIL: real '+++ b/foo.ts' header missing, got:"
  echo "${OUT}"
  exit 1
fi
echo "PASS: P1-9 scrub neutralizes embedded headers"

echo ""
echo "All synthesize_diff tests passed."
