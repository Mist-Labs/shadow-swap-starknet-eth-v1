import type { NextConfig } from "next"
import path from "path"

const nextConfig: NextConfig = {
  // Turbopack (Next.js 16 default — used by `pnpm dev`)
  // resolveAlias needs a project-root-relative path, NOT an absolute path
  turbopack: {
    resolveAlias: {
      "@react-native-async-storage/async-storage": "./lib/stubs/async-storage.js",
    },
  },

  // Webpack (used by `pnpm build` / production)
  // webpack requires the absolute path
  webpack(config) {
    config.resolve.alias = {
      ...config.resolve.alias,
      "@react-native-async-storage/async-storage": path.resolve(
        __dirname,
        "lib/stubs/async-storage.js"
      ),
    }
    return config
  },
}

export default nextConfig
