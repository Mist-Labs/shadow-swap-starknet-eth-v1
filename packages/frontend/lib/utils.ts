import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"
import { formatDistanceToNow } from "date-fns"
import { TOKENS, type TokenInfo } from "./tokens"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}


/**
 * Shared formatting and utility functions
 */

/**
 * Formats a date value (string, number, or Date) into a "time ago" string.
 */
export function formatTimeAgo(dateValue: string | number | undefined | Date): string {
  if (!dateValue) return "–"
  try {
    const date =
      typeof dateValue === "number"
        ? new Date(dateValue * 1000) // unix timestamp → ms
        : new Date(dateValue)
    if (isNaN(date.getTime())) return "–"
    return formatDistanceToNow(date, { addSuffix: true })
  } catch {
    return "–"
  }
}

/**
 * Formats a raw token amount string into a human-readable decimal string.
 */
export function formatAmount(amount: string, decimals: number = 6): string {
  try {
    const value = BigInt(amount)
    const divisor = BigInt(10 ** decimals)
    const quotient = value / divisor
    const remainder = value % divisor

    if (remainder === BigInt(0)) return quotient.toString()

    const remainderStr = remainder.toString().padStart(decimals, "0")
    const trimmed = remainderStr.slice(0, Math.min(6, decimals)).replace(/0+$/, "")

    return trimmed === "" ? quotient.toString() : `${quotient}.${trimmed}`
  } catch {
    return amount
  }
}

/**
 * Look up token metadata by its address across all chains.
 */
export function lookupTokenByAddress(address: string): TokenInfo | undefined {
  const lowerAddr = address.toLowerCase()
  
  // Search through all chains in the TOKENS registry
  for (const chain of Object.values(TOKENS)) {
    for (const token of Object.values(chain)) {
      if (token.address.toLowerCase() === lowerAddr) {
        return token
      }
    }
  }
  
  // Fallback for native ETH if not explicitly in registry or listed as zero address
  if (lowerAddr === "0x0000000000000000000000000000000000000000") {
    return TOKENS.ethereum.ETH
  }

  return undefined
}

/**
 * Returns the token symbol for a given address, with fallback logic.
 */
export function getTokenSymbol(address: string): string {
  if (!address.startsWith("0x")) return address // Already a symbol

  const tokenInfo = lookupTokenByAddress(address)
  if (tokenInfo) return tokenInfo.symbol

  // Ultimate fallback: truncated address
  return `${address.slice(0, 6)}...${address.slice(-4)}`
}

/**
 * Calculate success rate from stats
 */
export function calculateSuccessRate(completed: number, failed: number): number {
  const total = completed + failed;
  if (total === 0) return 0;
  return (completed / total) * 100;
}

/**
 * Formats large protocol numbers (e.g., 1.2M, 45K)
 */
export function formatLargeNumber(num: number): string {
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(2)}M`
  }
  if (num >= 1000) {
    return `${(num / 1000).toFixed(2)}K`
  }
  return num.toString()
}
