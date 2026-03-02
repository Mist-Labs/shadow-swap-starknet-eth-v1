import { useQuery } from "@tanstack/react-query"
import { formatDistanceToNow } from "date-fns"
import { listBridgeIntents } from "@/lib/api"
import type { IntentStatusResponse, IntentStatus } from "@/lib/api"
import type { ChainType } from "@/lib/tokens"

interface UseBridgeIntentsOptions {
  limit?: number
  status?: IntentStatus
  chain?: ChainType
  userAddress?: string
  /** Derived view key — when provided, backend will filter intents by this key */
  viewKey?: string
  refetchInterval?: number
}

/**
 * Fetches bridge intents from the real backend API via the /api/intents proxy.
 * All filters are passed through to the server — NO mock data.
 *
 * When viewKey is provided the query key changes so React Query re-fetches
 * automatically, and the proxy forwards view_key to the backend.
 */
export function useBridgeIntents(options: UseBridgeIntentsOptions = {}) {
  const { limit, status, chain, userAddress, viewKey, refetchInterval = 15_000 } = options

  const { data, isLoading, isError, error, refetch, isFetching } = useQuery({
    queryKey: ["bridgeIntents", { limit, status, chain, userAddress, viewKey }],
    queryFn: async () => {
      const result = await listBridgeIntents({ status, chain, limit, userAddress, viewKey })
      return result
    },
    refetchInterval,
    staleTime: 10_000,
    retry: 2,
  })

  const intents: IntentStatusResponse[] = data?.data ?? []
  const count = data?.count ?? 0

  return {
    intents,
    count,
    isLoading,
    isFetching,
    isError,
    error,
    refetch: async () => { await refetch() },
  }
}

export function formatChainName(chain: string) {
  if (chain === "ethereum") return "Ethereum"
  if (chain === "starknet") return "Starknet"
  return chain
}

export function formatTimeAgo(dateValue: string | number | undefined): string {
  if (!dateValue) return "–"
  try {
    const date =
      typeof dateValue === "number"
        ? new Date(dateValue * 1000)  // unix timestamp → ms
        : new Date(dateValue)
    if (isNaN(date.getTime())) return "–"
    return formatDistanceToNow(date, { addSuffix: true })
  } catch {
    return "–"
  }
}

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
