FROM node:18-alpine

WORKDIR /app

COPY packages/indexers/evm/package.json packages/indexers/evm/package-lock.json ./
COPY packages/indexers/evm/tsconfig.json ./

RUN npm ci

COPY packages/indexers/evm/src ./src
COPY packages/indexers/evm/abis ./abis

RUN npm run build

EXPOSE 3000

CMD ["npm", "start"]