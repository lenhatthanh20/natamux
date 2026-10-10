# Development image. It holds the tools; the source code is mounted from the codespace.
FROM node:22-bookworm-slim

# python3, make and g++ build node-pty when no prebuilt binary fits.
RUN apt-get update \
  && apt-get install -y --no-install-recommends git ca-certificates python3 make g++ \
  && rm -rf /var/lib/apt/lists/*

# The GitHub CLI opens pull requests, and gives git the credentials for a push (GH_TOKEN).
RUN apt-get update \
  && apt-get install -y --no-install-recommends curl \
  && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    -o /usr/share/keyrings/githubcli-archive-keyring.gpg \
  && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    > /etc/apt/sources.list.d/github-cli.list \
  && apt-get update \
  && apt-get install -y --no-install-recommends gh \
  && rm -rf /var/lib/apt/lists/*

# Same pnpm as the packageManager field of package.json.
ENV COREPACK_HOME=/opt/corepack
RUN corepack enable && corepack prepare pnpm@10.33.0 --activate

# `docker compose exec daemon natamux task list`
RUN printf '#!/bin/sh\nexec node /workspaces/natamux/packages/cli/dist/main.js "$@"\n' \
    > /usr/local/bin/natamux \
  && chmod +x /usr/local/bin/natamux \
  && ln -s natamux /usr/local/bin/nmx

WORKDIR /workspaces/natamux
