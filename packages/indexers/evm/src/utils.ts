import { GoldskyWebhookPayload, Chain } from "./types";

/**
 * Derive chain ID from Goldsky payload
 */
// utils.ts
export function deriveChainId(payload: GoldskyWebhookPayload): string {
  if (payload.metadata?.chain_id) {
    return payload.metadata.chain_id;
  }

  const eventData = payload.data.new;
  if (eventData.chain_id) {
    return eventData.chain_id;
  }

  // Goldsky doesn't include chain in payload — fall back to env
  if (process.env.CHAIN_ID) {
    return process.env.CHAIN_ID;
  }

  return "unknown";
}

/**
 * Get chain name from chain ID
 */
export function getChainName(chainId: string): Chain {
  switch (chainId) {
    case "1":
    case "ethereum":
      return Chain.Ethereum;
    case "11155111":
    case "sepolia":
      return Chain.Sepolia;
    default:
      throw new Error(`Unsupported chain ID: ${chainId}`);
  }
}

/**
 * Normalize hex string (add 0x prefix if missing)
 */
export function normalizeHex(value: string): string {
  if (!value) return value;
  let hex = value.toLowerCase();
  if (!hex.startsWith("0x")) {
    hex = "0x" + hex;
  }
  return hex;
}
