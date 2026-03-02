import type { NextConfig } from "next"
import path from "path"

const nextConfig: NextConfig = {
  webpack(config, { isServer }) {
    // @metamask/sdk (an optional dep via @reown/appkit-adapter-wagmi) tries to
    // import @react-native-async-storage/async-storage which doesn't exist in
    // a browser/Node environment. Stub it out with an empty module so the build
    // succeeds without warnings.
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
