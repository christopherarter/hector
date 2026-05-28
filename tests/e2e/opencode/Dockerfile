FROM hector-e2e-base:latest

# OpenCode bundles Bun, but we install it explicitly to keep image content
# deterministic. Bun ships as a static binary; non-root user can install
# under $HOME.
ENV BUN_INSTALL=/home/hector/.bun
ENV PATH=$BUN_INSTALL/bin:$PATH

USER root
RUN apt-get update -qq && apt-get install -y --no-install-recommends unzip && rm -rf /var/lib/apt/lists/*
USER hector

RUN curl -fsSL https://bun.sh/install | bash \
 && bun --version

USER root
RUN npm install -g opencode-ai \
 && opencode --version
USER hector

# Stage the plugin source. drive.sh wires it into the project's local
# .opencode/plugins/ directory at runtime — the adapter README documents
# this as the supported install path.
COPY --chown=hector:hector adapters/opencode/ /home/hector/opencode-plugin/

ENTRYPOINT ["bash", "/work/drive.sh"]
