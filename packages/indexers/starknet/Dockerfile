FROM node:20-slim

WORKDIR /app

# Install pnpm + build tools for native modules
RUN apt-get update && apt-get install -y python3 make g++ && rm -rf /var/lib/apt/lists/*
RUN npm install -g pnpm

# Copy root workspace files
COPY package.json pnpm-lock.yaml ./
COPY pnpm-workspace.yaml ./

# Copy the indexer package
COPY packages/indexers/starknet/package.json ./packages/indexers/starknet/
COPY packages/indexers/starknet/tsconfig.json ./packages/indexers/starknet/
COPY packages/indexers/starknet/apibara.config.ts ./packages/indexers/starknet/
COPY packages/indexers/starknet/drizzle.config.ts ./packages/indexers/starknet/

# Install dependencies but skip lifecycle scripts (prepare runs too early)
RUN pnpm install --frozen-lockfile --ignore-scripts

# Copy source code BEFORE running prepare
COPY packages/indexers/starknet/lib ./packages/indexers/starknet/lib
COPY packages/indexers/starknet/indexers ./packages/indexers/starknet/indexers
COPY packages/indexers/starknet/utils ./packages/indexers/starknet/utils

# Now run prepare and build
RUN pnpm --filter @shadowswap/indexer-starknet run prepare
RUN pnpm --filter @shadowswap/indexer-starknet run build

WORKDIR /app/packages/indexers/starknet
CMD ["/app/packages/indexers/starknet/node_modules/.bin/apibara", "start", "--indexer", "shadowswap"]