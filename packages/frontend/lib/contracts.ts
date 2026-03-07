import type { ChainType } from "./tokens"
import type { Address } from "viem"

export const INTENT_POOL_ABI = [] as const;
export const ETHEREUM_CONTRACTS = {
    intentPool: "0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468" as Address,
}
export const STARKNET_CONTRACTS = {
    intentPool: "0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0" as Address,
}

export function getTxUrl(chain: ChainType, txHash: string) {
    if (chain === "ethereum") {
        return `https://etherscan.io/tx/${txHash}`
    }
    if (chain === "starknet") {
        return `https://starkscan.co/tx/${txHash}`
    }
    return "#"
}
