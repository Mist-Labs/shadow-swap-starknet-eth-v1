export type ChainType = "ethereum" | "starknet"

export interface TokenInfo {
    name: string
    symbol: string
    decimals: number
    logo: string
    address: string
    /** NEAR asset identifier for this token. Undefined if token requires a pre-swap (multicall). */
    nearAssetId?: string
    /**
     * Tokens that require an AVNU multicall to STRK before bridging via NEAR.
     * NEAR only supports STRK from StarkNet; ETH/USDT/USDC all need a pre-swap.
     */
    requiresMulticall?: boolean
}

/** Tokens available when bridging FROM StarkNet → Ethereum */
// NEAR only supports STRK from StarkNet. ETH/USDT/USDC require a multicall to STRK first.
export const STARKNET_SOURCE_TOKENS = ["STRK", "ETH", "USDC", "USDT"]

/** Tokens available when bridging FROM Ethereum → StarkNet */
export const ETHEREUM_SOURCE_TOKENS = ["ETH", "USDC", "USDT"]

/** Returns the direction-aware token list for a given source chain */
export function getSupportedTokens(sourceChain: ChainType): string[] {
    return sourceChain === "starknet" ? STARKNET_SOURCE_TOKENS : ETHEREUM_SOURCE_TOKENS
}

export const TOKENS: Record<ChainType, Record<string, TokenInfo>> = {
    ethereum: {
        ETH: {
            name: "Ethereum",
            symbol: "ETH",
            decimals: 18,
            logo: "/ethereum_logo.png",
            address: "0x0000000000000000000000000000000000000000",
            nearAssetId: "nep141:eth.omft.near",
        },
        USDC: {
            name: "USD Coin",
            symbol: "USDC",
            decimals: 6,
            logo: "/usdc-logo.webp",
            address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            nearAssetId: "nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near",
        },
        USDT: {
            name: "Tether USD",
            symbol: "USDT",
            decimals: 6,
            logo: "/USDT-logo.png",
            address: "0xdAC17F958D2ee523a2206206994597C13D831ec7",
            nearAssetId: "nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near",
        },
    },
    starknet: {
        STRK: {
            name: "Starknet Token",
            symbol: "STRK",
            decimals: 18,
            logo: "/Starknet-logo.png",
            address: "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
            nearAssetId: "nep141:starknet.omft.near",
        },
        ETH: {
            name: "Ether",
            symbol: "ETH",
            decimals: 18,
            logo: "/ethereum_logo.png",
            address: "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
            nearAssetId: "nep141:starknet-eth.omft.near",
            // NEAR only supports STRK from StarkNet — ETH must be swapped to STRK first
            requiresMulticall: true,
        },
        USDC: {
            name: "USD Coin",
            symbol: "USDC",
            decimals: 6,
            logo: "/usdc-logo.webp",
            address: "0x053c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8",
            nearAssetId: "nep141:starknet-usdc.omft.near",
            // NEAR does not support USDC from StarkNet — must swap to STRK first
            requiresMulticall: true,
        },
        USDT: {
            name: "Tether USD",
            symbol: "USDT",
            decimals: 6,
            logo: "/USDT-logo.png",
            address: "0x068f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8",
            nearAssetId: "nep141:starknet-usdt.omft.near",
            // NEAR does not support USDT from StarkNet — must swap to STRK first
            requiresMulticall: true,
        },
    },
}

export const getTokenInfo = (symbol: string, chain: ChainType): TokenInfo | undefined =>
    TOKENS[chain]?.[symbol]

/** STRK token on Starknet — used as the post-swap token in multicall flows */
export const STRK_TOKEN = TOKENS.starknet.STRK!

/**
 * Look up token metadata by its address across all chains.
 */
export function lookupTokenByAddress(address: string): TokenInfo | undefined {
    const lowerAddr = address.toLowerCase()
    for (const chain of Object.values(TOKENS)) {
        for (const token of Object.values(chain)) {
            if (token.address.toLowerCase() === lowerAddr) {
                return token
            }
        }
    }
    return undefined
}

/**
 * Validates that the selected destination token is compatible with the destination chain.
 * Prevents "Wrong Chain Delivery" by catching mismatches before bridge initiation.
 */
export function validateAssetChainPair(
    destChain: ChainType,
    nearAssetId: string
): void {
    const isStarknetAsset = nearAssetId.includes("starknet")
    const isEvmAsset = nearAssetId.includes("eth")

    if (destChain === "starknet" && isEvmAsset) {
        throw new Error(
            `Destination chain is Starknet but the selected asset is an Ethereum asset (${nearAssetId}).`
        )
    }
    if (destChain === "ethereum" && isStarknetAsset) {
        throw new Error(
            `Destination chain is Ethereum but the selected asset is a Starknet asset (${nearAssetId}).`
        )
    }
}
