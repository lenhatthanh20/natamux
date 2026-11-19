# The daemon image for CI and codespace tests (scripts/e2e.sh). Not a supported self-host.

FROM rust@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 AS rust-build
RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential git pkg-config \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo .cargo
COPY crates crates
COPY xtask xtask
# As release.yml: the debug info of the release profile only slows the build.
ENV CARGO_PROFILE_RELEASE_DEBUG=0
RUN cargo build --release -p natamux-daemon -p natamux-cli --locked

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 AS daemon
LABEL org.opencontainers.image.source="https://github.com/lenhatthanh20/natamux"
RUN apt-get update \
  && apt-get install -y --no-install-recommends bash ca-certificates curl git \
  && rm -rf /var/lib/apt/lists/* \
  && groupadd -g 1000 natamux \
  && useradd -u 1000 -g 1000 -m natamux \
  && mkdir -p /data \
  && chown natamux:natamux /data
COPY --from=rust-build /app/target/release/natamux-daemon /usr/local/bin/natamux-daemon
COPY --from=rust-build /app/target/release/natamux /usr/local/bin/natamux
COPY --from=rust-build /app/target/release/nmx /usr/local/bin/nmx
ENV NATAMUX_HOST=0.0.0.0 \
    NATAMUX_DATA_DIR=/data \
    NATAMUX_REPO_DIR=/repo \
    NATAMUX_WORKTREES_DIR=/data/worktrees \
    NATAMUX_BIN_DIR=/usr/local/bin \
    HOME=/tmp
USER natamux
WORKDIR /app
EXPOSE 4317
CMD ["/usr/local/bin/natamux-daemon"]
