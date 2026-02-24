import { GoldskyWebhookPayload, Chain } from "./types";

/**
 * Derive chain ID from Goldsky payload
 */
export function deriveChainId(payload: GoldskyWebhookPayload): string {
  // Try metadata first
  if (payload.metadata?.chain_id) {
    return payload.metadata.chain_id;
  }

  // Try event data
  const eventData = payload.data.new;
  if (eventData.chain_id) {
    return eventData.chain_id;
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
