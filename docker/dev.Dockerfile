# Development image. It holds the tools; the source code is mounted from the codespace.
FROM node:22-bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends git ca-certificates \
  && rm -rf /var/lib/apt/lists/*

# Same pnpm as the packageManager field of package.json.
ENV COREPACK_HOME=/opt/corepack
RUN corepack enable && corepack prepare pnpm@10.33.0 --activate

# `docker compose exec daemon helmsman task list`
RUN printf '#!/bin/sh\nexec node /workspaces/helmsman/packages/cli/dist/main.js "$@"\n' \
    > /usr/local/bin/helmsman \
  && chmod +x /usr/local/bin/helmsman

WORKDIR /workspaces/helmsman
