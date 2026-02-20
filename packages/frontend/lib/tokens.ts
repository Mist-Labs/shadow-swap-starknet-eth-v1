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
      address: "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238", // Sepolia
    },
    USDT: {
      name: "Tether USD",
      symbol: "USDT",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/tether-usdt-logo.png",
      address: "0xaA8E23Fb1079EA71e0a56F48a2aA51851D8433D0", // Sepolia
    },
  },
  starknet: {
    ETH: {
      name: "Ether",
      symbol: "ETH",
      decimals: 18,
      logo: "https://cryptologos.cc/logos/ethereum-eth-logo.png",
      address: "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7", // Sepolia
    },
    USDC: {
      name: "USD Coin",
      symbol: "USDC",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/usd-coin-usdc-logo.png",
      address: "0x053c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8", // Sepolia
    },
    USDT: {
      name: "Tether USD",
      symbol: "USDT",
      decimals: 6,
      logo: "https://cryptologos.cc/logos/tether-usdt-logo.png",
      address: "0x0", // TODO: Add Starknet Sepolia USDT
    },
  },
}

export const getTokenInfo = (symbol: string, chain: ChainType): TokenInfo | undefined => {
  return TOKENS[chain]?.[symbol]
}
