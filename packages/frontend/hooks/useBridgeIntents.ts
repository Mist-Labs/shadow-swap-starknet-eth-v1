import { useQuery } from "@tanstack/react-query"
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

