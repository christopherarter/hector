# syntax=docker/dockerfile:1
#
# Onboarding feature test image for `hector init`.
#
# The host is typically macOS, so a host-built `hector` is a Mach-O binary that
# can't run in a Linux container. We therefore compile a Linux `hector` from
# source in a build stage, then drop it into a minimal runtime stage.
#
# Build context MUST be the repo root: the adapter registry embeds the
# adapters/<harness>/ artifacts via include_str!, so adapters/ has to be present
# at compile time. The COPYs below are ordered so editing the test scripts does
# not invalidate the (slow) cargo build layer.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml ./
COPY crates ./crates
COPY adapters ./adapters
RUN cargo build --release -p hector-cli && cp target/release/hector /hector

# Minimal runtime: `hector init` writes files and patches JSON itself (in Rust),
# and this test never executes the harnesses — so no jq/node/bun/real harnesses
# are needed, only the binary, bash, and coreutils (all in debian-slim).
FROM debian:bookworm-slim
RUN useradd --create-home --uid 1000 tester
COPY --from=build /hector /usr/local/bin/hector
COPY tests/e2e/init/drive.sh /usr/local/bin/drive.sh
USER tester
ENTRYPOINT ["bash", "/usr/local/bin/drive.sh"]
