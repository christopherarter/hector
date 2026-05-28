#!/usr/bin/env bash
# Drive script for the reasonix adapter e2e harness.
#
# Deviation from plan: adapters/reasonix/hooks/settings.example.json contains
# a fully-qualified developer-local path in the command field
# ("/Users/chrisarter/Documents/projects/hector/adapters/reasonix/hooks/hook.sh")
# rather than a "<plugin-root>" placeholder.  The sed substitution therefore
# replaces that entire host-side prefix with the container-staged path
# /home/hector/reasonix-plugin so the resulting settings.json is correct
# inside the container.

set -uo pipefail

DRIVE_LOG="/work/runs/drive.log"
HARNESS_LOG="/work/runs/harness.log"
mkdir -p /work/runs/.hector

log()  { printf "[%s] %s\n" "$(date -u +%H:%M:%S)" "$*" | tee -a "$DRIVE_LOG"; }
fail() { log "LIFECYCLE FAIL: $*"; exit 1; }

CASE=""
for arg in "$@"; do
  case "$arg" in
    --case=*) CASE="${arg#--case=}" ;;
    *) fail "unknown arg: $arg" ;;
  esac
done
[[ -n "$CASE" ]] || fail "missing --case=<name>"

CASE_FILE="/work/cases/$CASE.json"
[[ -f "$CASE_FILE" ]] || fail "case file not found: $CASE_FILE"

log "phase 0: setup; case=$CASE"
[[ -n "${ANTHROPIC_API_KEY:-}" ]] || fail "ANTHROPIC_API_KEY not in environment"
[[ -x /usr/local/bin/hector  ]] || fail "/usr/local/bin/hector not executable"

PROMPT="$(jq -r '.prompt' "$CASE_FILE")"
TARGET_FILE="$(jq -r '.target_file' "$CASE_FILE")"

log "phase 1: install check"
hector --version   | tee -a "$DRIVE_LOG" || fail "hector --version"
reasonix --version | tee -a "$DRIVE_LOG" || fail "reasonix --version"
[[ -d /home/hector/reasonix-plugin/hooks ]] || fail "plugin hooks dir missing"

log "phase 2: onboarding"
# Wire the PreToolUse hook into ~/.reasonix/settings.json. The adapter ships
# hooks/settings.example.json as a template; we rewrite its hardcoded
# developer-local path prefix to the absolute staged path inside the container.
# (The example file contains no "<plugin-root>" placeholder — it uses the
# author's host path verbatim.  See deviation note at top of this file.)
mkdir -p /home/hector/.reasonix
if [[ -f /home/hector/reasonix-plugin/hooks/settings.example.json ]]; then
  sed 's|/Users/chrisarter/Documents/projects/hector/adapters/reasonix|/home/hector/reasonix-plugin|g' \
    /home/hector/reasonix-plugin/hooks/settings.example.json \
    >/home/hector/.reasonix/settings.json
else
  fail "hooks/settings.example.json missing in adapter"
fi
log "wired settings.json:"
cat /home/hector/.reasonix/settings.json | tee -a "$DRIVE_LOG"

WORKDIR=/work/runs/workdir
mkdir -p "$WORKDIR" && cd "$WORKDIR" || fail "cd workdir"
git init -q
cp -r /work/fixture/. "$WORKDIR/"
git add -A && git -c user.email=e2e@hector -c user.name=e2e commit -q -m "fixture"

hector init >"$DRIVE_LOG.init.out" 2>&1 || fail "hector init"
cp .hector.yml /work/runs/.hector.yml.from-init 2>/dev/null || true
cp /work/policy/.hector.yml ./.hector.yml
hector trust    | tee -a "$DRIVE_LOG" || fail "hector trust"
hector validate | tee -a "$DRIVE_LOG" || fail "hector validate"

log "phase 3: drive harness with reasonix --headless"
timeout 120 reasonix --headless --message "$PROMPT" \
  >>"$HARNESS_LOG" 2>&1
HARNESS_EXIT=$?
log "harness exit: $HARNESS_EXIT"

log "phase 4: capture forensics"
if [[ -f "$WORKDIR/.hector/log.jsonl" ]]; then
  cp "$WORKDIR/.hector/log.jsonl" /work/runs/.hector/log.jsonl
fi
if [[ -f /work/runs/.hector/log.jsonl ]]; then
  tail -n 50 /work/runs/.hector/log.jsonl \
    | jq -s 'last' >/work/runs/verdict.json 2>/dev/null || true
fi

log "phase 5: lifecycle complete"
exit 0
