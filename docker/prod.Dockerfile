# Production images: a Rust daemon with the dashboard embedded, and nginx for port 8080.

FROM node:22-bookworm-slim AS web-build
ENV COREPACK_HOME=/opt/corepack
RUN corepack enable && corepack prepare pnpm@10.33.0 --activate
WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY packages/core/package.json packages/core/
COPY packages/web/package.json packages/web/
RUN pnpm install --frozen-lockfile
COPY packages/core packages/core
COPY packages/web packages/web
RUN pnpm --filter @natamux/core --filter @natamux/web build

FROM rust@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 AS rust-build
RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential git pkg-config \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo .cargo
COPY crates crates
COPY xtask xtask
COPY --from=web-build /app/packages/web/dist packages/web/dist
ENV NATAMUX_REQUIRE_WEB=1
RUN cargo build --release -p natamux-daemon -p natamux-cli --locked

FROM debian:trixie-slim AS daemon
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
CMD ["natamux-daemon"]

FROM nginxinc/nginx-unprivileged:stable-alpine AS web
LABEL org.opencontainers.image.source="https://github.com/lenhatthanh20/natamux"
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=web-build /app/packages/web/dist /usr/share/nginx/html
EXPOSE 8080
