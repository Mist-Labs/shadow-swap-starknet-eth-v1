import { createAppKit } from "@reown/appkit/react"
import { WagmiAdapter } from "@reown/appkit-adapter-wagmi"
import { mainnet } from "@reown/appkit/networks"
import { QueryClient } from "@tanstack/react-query"

const projectId = process.env.NEXT_PUBLIC_REOWN_PROJECT_ID || ""

if (!projectId) {
  console.warn(
    "NEXT_PUBLIC_REOWN_PROJECT_ID is not set. EVM wallet connection will not work."
  )
}

const metadata = {
  name: "Shadow Swap",
  description: "Privacy-preserving cross-chain bridge between Starknet and Ethereum",
  url: typeof window !== "undefined" ? window.location.origin : "https://shadowswap.xyz",
  icons: ["/icon.svg"],
}

export const queryClient = new QueryClient()

export const wagmiAdapter = new WagmiAdapter({
  networks: [mainnet],
  projectId,
  ssr: true,
})

// Initialise the Reown AppKit modal (side-effect; called once at module level)
export const modal = createAppKit({
  adapters: [wagmiAdapter],
  networks: [mainnet],
  projectId,
  metadata,
  features: {
    analytics: false,
    email: false,
    socials: [],
  },
  themeMode: "dark",
  themeVariables: {
    "--w3m-accent": "#f97316", // orange-500 — matches theme
    "--w3m-border-radius-master": "8px",
  },
})

// Export wagmi config consumed by WagmiProvider
export const config = wagmiAdapter.wagmiConfig
