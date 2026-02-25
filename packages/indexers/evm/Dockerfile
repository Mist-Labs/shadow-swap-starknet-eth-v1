FROM node:18-alpine

WORKDIR /app

COPY package.json package-lock.json ./
COPY tsconfig.json ./

RUN npm ci

COPY src ./src
COPY abis ./abis

RUN npm run build

EXPOSE 3000

CMD ["npm", "start"]