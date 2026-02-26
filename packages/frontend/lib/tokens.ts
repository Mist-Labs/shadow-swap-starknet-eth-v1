export type ChainType = "ethereum" | "starknet"

export interface TokenInfo {
  name: string
  symbol: string
  decimals: number
  logo: string
  address: string
}

export const SUPPORTED_TOKENS = ["ETH", "USDC", "USDT"]

export const TOKENS: Record<ChainType, Record<string, TokenInfo>> = {
  ethereum: {
    ETH: {
      name: "Ethereum",
      symbol: "ETH",
      decimals: 18,
      logo: "https://cryptologos.cc/logos/ethereum-eth-logo.png",
      address: "0x0000000000000000000000000000000000000000",
    },
    USDC: {
      name: "USD Coin",
      symbol: "USDC",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/usd-coin-usdc-logo.png",
      address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", // Mainnet
    },
    USDT: {
      name: "Tether USD",
      symbol: "USDT",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/tether-usdt-logo.png",
      address: "0xdAC17F958D2ee523a2206206994597C13D831ec7", // Mainnet
    },
  },
  starknet: {
    ETH: {
      name: "Ether",
      symbol: "ETH",
      decimals: 18,
      logo: "https://cryptologos.cc/logos/ethereum-eth-logo.png",
      address: "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7", // Mainnet
    },
    USDC: {
      name: "USD Coin",
      symbol: "USDC",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/usd-coin-usdc-logo.png",
      address: "0x053c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8", // Mainnet
    },
    USDT: {
      name: "Tether USD",
      symbol: "USDT",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/tether-usdt-logo.png",
      address: "0x068f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8", // Mainnet
    },
  },
}

export const getTokenInfo = (symbol: string, chain: ChainType): TokenInfo | undefined => {
  return TOKENS[chain]?.[symbol]
}
