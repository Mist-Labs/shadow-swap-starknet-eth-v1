FROM node:20-alpine

WORKDIR /app

# Copy package files
COPY package.json ./
COPY tsconfig.json ./
COPY apibara.config.ts ./
COPY drizzle.config.ts ./

# Install dependencies
RUN npm install

# Copy source code
COPY lib ./lib
COPY indexers ./indexers
COPY utils ./utils

# Build
RUN npm run build

# Start indexer
CMD ["npm", "start"]
