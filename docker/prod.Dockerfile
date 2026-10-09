# Production images: `daemon` (the task service) and `web` (the dashboard behind nginx).
# Build one with --target daemon or --target web. compose.prod.yaml builds both.

FROM node:22-bookworm-slim AS base
# python3, make and g++ build node-pty when no prebuilt binary fits. They stay out of the
# final images.
RUN apt-get update \
  && apt-get install -y --no-install-recommends python3 make g++ \
  && rm -rf /var/lib/apt/lists/*
ENV COREPACK_HOME=/opt/corepack
RUN corepack enable && corepack prepare pnpm@10.33.0 --activate
WORKDIR /app

# Only the manifests, so the install layer is reused until a dependency changes.
FROM base AS manifests
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY packages/core/package.json packages/core/
COPY packages/daemon/package.json packages/daemon/
COPY packages/cli/package.json packages/cli/
COPY packages/web/package.json packages/web/
COPY packages/desktop/package.json packages/desktop/
# The images serve the dashboard in a browser; the desktop app's runtime is not needed.
ENV ELECTRON_SKIP_BINARY_DOWNLOAD=1

FROM manifests AS build
RUN pnpm install --frozen-lockfile
COPY packages packages
RUN pnpm --filter @helmsman/core --filter @helmsman/daemon --filter @helmsman/web build

# The daemon and what it needs, without dev dependencies.
FROM manifests AS daemon-deps
RUN pnpm install --frozen-lockfile --prod --filter "@helmsman/daemon..."

FROM node:22-bookworm-slim AS daemon
# git is how the daemon makes the worktree of a task.
RUN apt-get update \
  && apt-get install -y --no-install-recommends git ca-certificates \
  && rm -rf /var/lib/apt/lists/*
ENV NODE_ENV=production \
    HELMSMAN_HOST=0.0.0.0 \
    HELMSMAN_DATA_DIR=/data \
    HELMSMAN_REPO_DIR=/repo \
    HELMSMAN_WORKTREES_DIR=/data/worktrees \
    HOME=/tmp
WORKDIR /app
COPY --from=daemon-deps /app /app
COPY --from=build /app/packages/core/dist packages/core/dist
COPY --from=build /app/packages/daemon/dist packages/daemon/dist
RUN mkdir -p /data && chown node:node /data
USER node
EXPOSE 4317
CMD ["node", "packages/daemon/dist/main.js"]

FROM nginxinc/nginx-unprivileged:stable-alpine AS web
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/packages/web/dist /usr/share/nginx/html
EXPOSE 8080
