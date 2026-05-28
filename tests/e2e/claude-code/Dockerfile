FROM hector-e2e-base:latest

# Claude Code CLI — installed globally as root, then we drop back to `hector`.
USER root
RUN npm install -g @anthropic-ai/claude-code \
 && claude --version
USER hector

# Plugin source — copied into the conventional plugin path. CLAUDE_PLUGIN_ROOT
# is set by Claude Code at hook-fire time; install location is taken from
# the adapter README. If Claude Code's plugin discovery rejects this path,
# the fix is a one-line `COPY` destination change here.
COPY --chown=hector:hector adapters/claude-code/ /home/hector/.claude/plugins/hector/

ENTRYPOINT ["bash", "/work/drive.sh"]
