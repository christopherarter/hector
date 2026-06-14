#!/usr/bin/env bash
set -euo pipefail

# Claude Code adapter for hector.
#
# Gates Edit/Write edits via the PostToolUse hook: synthesize a unified diff
# from the event's (old_string, new_string), then run `hector check --diff`
# against it. Exit codes:
#   - 2 = block (verdict JSON on stderr),
#   - 3 = engine internal error (fail-open by default; set
#         HECTOR_FAIL_CLOSED_ON_INTERNAL=1 to block instead),
#   - 0 = pass/warn.
#
# Event JSON arrives on stdin. We pipe through jq to extract paths. A first
# positional argument (the hook event name) is accepted but ignored — only
# the post-tool-use gate remains.

# Default project root is the CWD where Claude Code is running.
PROJECT_ROOT="$(pwd)"
CONFIG="${PROJECT_ROOT}/.hector.yml"
HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SYNTHESIZE_DIFF="${HOOK_DIR}/synthesize_diff.sh"

# Per-invocation temp files for verdict + diff; cleaned up on exit so
# concurrent Claude Code sessions don't clobber each other.
TMP_VERDICT=""
TMP_DIFF=""
cleanup() {
  if [[ -n "${TMP_VERDICT}" && -f "${TMP_VERDICT}" ]]; then
    rm -f "${TMP_VERDICT}"
  fi
  if [[ -n "${TMP_DIFF}" && -f "${TMP_DIFF}" ]]; then
    rm -f "${TMP_DIFF}"
  fi
}
trap cleanup EXIT

# Skip silently if hector isn't configured for this project.
if [[ ! -f "${CONFIG}" ]]; then
  exit 0
fi

# Parse the event JSON for the changed file.
EVENT=$(cat)
FILE=$(echo "${EVENT}" | jq -r '.tool_input.file_path // .tool_input.path // empty')
if [[ -z "${FILE}" ]]; then
  # No file in event payload — nothing to check.
  exit 0
fi

# R3: short-circuit on edits to the policy file itself. The on-disk sha will
# not match `trust:` while the user is mid-edit; any `hector` invocation would
# fail the trust gate and surface a misleading "internal error" to the user.
# Match by basename so the skip works for both relative and absolute paths
# Claude Code may send.
BASENAME="${FILE##*/}"
if [[ "${BASENAME}" == ".hector.yml" || "${BASENAME}" == ".bully.yml" ]]; then
  exit 0
fi

# Compute the changed file's path relative to the project root. Unified diffs
# use repo-relative `a/`/`b/` headers, and `hector check --diff` rejects
# absolute and `..`-escaping paths (P0-4). Claude Code sends absolute
# file_paths, so relativize here. Resolve both sides to their physical
# (symlink-free) form first: on macOS $TMPDIR lives under /var, a symlink to
# /private/var, so a naive string prefix would miss. When the file isn't
# cleanly inside the project, REL stays empty and we gate on the on-disk file
# instead (an out-of-project file matches no repo-relative rule scope anyway).
ROOT_PHYS=$(pwd -P)
REL=""
FILE_DIR=$(cd "$(dirname "${FILE}")" 2>/dev/null && pwd -P || true)
if [[ -n "${FILE_DIR}" ]]; then
  FILE_PHYS="${FILE_DIR}/$(basename "${FILE}")"
  case "${FILE_PHYS}" in
    "${ROOT_PHYS}/"*) REL="${FILE_PHYS#"${ROOT_PHYS}/"}" ;;
  esac
fi

# Build a synthetic unified diff. Claude Code's Edit/Write events don't carry
# a real diff, so we fake one from the (old_string, new_string) pair. The
# synthesizer emits correct `@@ -1,N +1,M @@` counts for multi-line edits and
# escapes any OLD/NEW line that looks like a diff header, so attacker-
# controlled content can't reframe the diff onto another file. The header uses
# REL when available so the diff is a valid `hector check --diff` input.
OLD=$(echo "${EVENT}" | jq -r '.tool_input.old_string // ""')
NEW=$(echo "${EVENT}" | jq -r '.tool_input.new_string // .tool_input.content // ""')
DIFF=$("${SYNTHESIZE_DIFF}" "${REL:-${FILE}}" "${OLD}" "${NEW}")

# Gate input: prefer the synthesized diff so AST rules and the diff-relevance
# skip see the change in context. Deterministic script/ast rules read the
# on-disk file in diff mode either way, so this is a strict superset of
# `--file`. Fall back to `--file` when the path couldn't be relativized.
if [[ -n "${REL}" ]]; then
  TMP_DIFF=$(mktemp -t hector-diff.XXXXXX)
  printf '%s\n' "${DIFF}" > "${TMP_DIFF}"
  GATE_INPUT=(--diff "${TMP_DIFF}")
else
  GATE_INPUT=(--file "${FILE}")
fi

# Gate the edit. Differentiate hector exit codes:
#   0 = pass/warn, 2 = block, 3 = engine internal error, 1 = config/load error.
# Suppress hector's own stderr so the verdict JSON we cat to stderr on block
# (exit 2) parses cleanly. The macOS capability sandbox emits a per-process
# advisory warning that would otherwise prepend to the verdict stream.
TMP_VERDICT=$(mktemp -t hector-verdict.XXXXXX)
EC=0
hector check "${GATE_INPUT[@]}" --config "${CONFIG}" --format json > "${TMP_VERDICT}" 2>/dev/null || EC=$?
case "${EC}" in
  0) exit 0 ;;
  2)
    cat "${TMP_VERDICT}" >&2
    exit 2
    ;;
  3)
    # Engine internal error (script spawn failure, etc.). Fail-open by default
    # so a broken gate doesn't block the agent. Opt-in fail-closed:
    # HECTOR_FAIL_CLOSED_ON_INTERNAL=1.
    if [[ "${HECTOR_FAIL_CLOSED_ON_INTERNAL:-0}" == "1" ]]; then
      echo "hector: internal error — failing closed (HECTOR_FAIL_CLOSED_ON_INTERNAL=1)" >&2
      [[ -s "${TMP_VERDICT}" ]] && cat "${TMP_VERDICT}" >&2
      exit 2
    fi
    echo "hector: internal error during check — allowing edit; see .hector/log.jsonl" >&2
    exit 0
    ;;
  *)
    echo "hector: internal error checking ${FILE} (exit ${EC})" >&2
    [[ -s "${TMP_VERDICT}" ]] && cat "${TMP_VERDICT}" >&2
    exit 1
    ;;
esac
