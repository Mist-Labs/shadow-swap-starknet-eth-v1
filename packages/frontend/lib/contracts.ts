import type { ChainType } from "./tokens"
import type { Address } from "viem"

export const INTENT_POOL_ABI = [] as const;
export const ETHEREUM_CONTRACTS = { intentPool: "0x0000000000000000000000000000000000000000" as Address };
export const STARKNET_CONTRACTS = { intentPool: "0x0000000000000000000000000000000000000000" as Address };

export function getTxUrl(chain: ChainType, txHash: string) {
  if (chain === "ethereum") {
    return `https://sepolia.etherscan.io/tx/${txHash}`
  }
  if (chain === "starknet") {
    return `https://sepolia.starkscan.co/tx/${txHash}`
  }
  return "#"
}
