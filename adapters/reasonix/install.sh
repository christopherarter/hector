#!/usr/bin/env bash
set -euo pipefail

echo "DEPRECATED: install.sh is superseded by \`ironlint init --harness reasonix\`. This script is kept only as a fallback for environments without the ironlint binary." >&2
echo "Run \`ironlint init --harness reasonix\` instead for atomic writes, idempotency, and \`ironlint doctor\` integration." >&2

# Install or refresh the IronLint Reasonix PreToolUse hook in Reasonix settings.
#
# This keeps onboarding repeatable and also cleans up stale IronLint Reasonix
# entries from earlier adapter revisions, including non-gating PostToolUse
# hooks.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
HOOK_PATH="${SCRIPT_DIR}/hooks/hook.sh"
SETTINGS="${REASONIX_SETTINGS:-${HOME}/.reasonix/settings.json}"
BACKUP=""

if ! command -v jq >/dev/null 2>&1; then
  echo "ironlint reasonix installer: jq is required" >&2
  exit 1
fi

if [[ ! -f "${HOOK_PATH}" ]]; then
  echo "ironlint reasonix installer: hook not found at ${HOOK_PATH}" >&2
  exit 1
fi

mkdir -p "$(dirname -- "${SETTINGS}")"

INPUT=$(mktemp -t ironlint-reasonix-settings-in.XXXXXX)
OUTPUT=$(mktemp -t ironlint-reasonix-settings-out.XXXXXX)
cleanup() {
  rm -f "${INPUT}" "${OUTPUT}"
}
trap cleanup EXIT

if [[ -f "${SETTINGS}" ]]; then
  jq empty "${SETTINGS}"
  BACKUP="${SETTINGS}.bak-$(date +%Y%m%d%H%M%S)"
  cp "${SETTINGS}" "${BACKUP}"
  cp "${SETTINGS}" "${INPUT}"
else
  printf '{}\n' > "${INPUT}"
fi

jq --arg command "${HOOK_PATH} pre-tool-use" '
  def without_ironlint_reasonix:
    map(select(((.command // "") | contains("adapters/reasonix/hooks/hook.sh")) | not));

  .hooks = (.hooks // {})
  | .hooks.PostToolUse = ((.hooks.PostToolUse // []) | without_ironlint_reasonix)
  | .hooks.PreToolUse = (
      ((.hooks.PreToolUse // []) | without_ironlint_reasonix)
      + [{
          "command": $command,
          "match": "^(write_file|edit_file|multi_edit)$",
          "description": "Block edits that violate ironlint policy before they land on disk",
          "timeout": 30000
        }]
    )
' "${INPUT}" > "${OUTPUT}"

mv "${OUTPUT}" "${SETTINGS}"
trap - EXIT
rm -f "${INPUT}"

echo "Installed IronLint Reasonix hook in ${SETTINGS}"
if [[ -n "${BACKUP}" ]]; then
  echo "Backup written to ${BACKUP}"
fi
echo "Restart Reasonix so it reloads settings."
