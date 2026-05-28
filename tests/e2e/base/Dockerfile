# Shared base for all adapter e2e images.
# Adds system deps, Node LTS, and a non-root user.
# Leaves install their harness CLI on top.
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bash \
      ca-certificates \
      curl \
      git \
      gnupg \
      jq \
      procps \
      python3 \
 && rm -rf /var/lib/apt/lists/*

# Node.js LTS via NodeSource (current LTS major: 20).
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
 && apt-get update \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/* \
 && node --version \
 && npm --version

# Non-root user; uid 1000 matches the conventional host-user uid so bind-mount
# perms line up without further chowning.
RUN useradd -m -u 1000 -s /bin/bash hector \
 && mkdir -p /work /work/runs /work/cases /work/policy /work/fixture \
 && chown -R hector:hector /work

USER hector
WORKDIR /work
