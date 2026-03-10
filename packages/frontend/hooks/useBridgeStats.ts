/**
 * useBridgeStats Hook
 *
 * Hook for fetching bridge statistics and health data from the backend
 * using React Query for optimized state management and to avoid cascading renders.
 */

import { useQuery } from "@tanstack/react-query";
import { getBridgeStats, healthCheck } from "@/lib/api";
import { parseBridgeError } from "@/lib/errors";

/**
 * Hook for fetching bridge statistics
 */
export function useBridgeStats(refreshInterval: number = 30000) {
  const query = useQuery({
    queryKey: ["bridgeStats"],
    queryFn: async () => {
      const response = await getBridgeStats();
      return {
        totalIntents: response.data.total_intents,
        pendingIntents: response.data.pending_intents,
        filledIntents: response.data.filled_intents ?? response.data.settled_intents ?? 0,
        completedIntents: response.data.completed_intents ?? response.data.settled_intents ?? 0,
        failedIntents: response.data.failed_intents,
        refundedIntents: response.data.refunded_intents,
        ethereumToStarknet: response.data.ethereum_to_starknet ?? 0,
        starknetToEthereum: response.data.starknet_to_ethereum ?? 0,
        volumeByToken: response.data.total_volume_by_token ?? {},
      };
    },
    refetchInterval: refreshInterval,
    staleTime: 10000,
  });

  return {
    ...query.data, // This spreads the stats fields
    isLoading: query.isLoading,
    error: query.error ? parseBridgeError(query.error) : null,
    refetch: query.refetch,
    // Provide default values when data is loading/missing
    totalIntents: query.data?.totalIntents ?? 0,
    pendingIntents: query.data?.pendingIntents ?? 0,
    filledIntents: query.data?.filledIntents ?? 0,
    completedIntents: query.data?.completedIntents ?? 0,
    failedIntents: query.data?.failedIntents ?? 0,
    refundedIntents: query.data?.refundedIntents ?? 0,
    ethereumToStarknet: query.data?.ethereumToStarknet ?? 0,
    starknetToEthereum: query.data?.starknetToEthereum ?? 0,
    volumeByToken: query.data?.volumeByToken ?? {},
  };
}

/**
 * Hook for health check
 */
export function useHealthCheck(refreshInterval: number = 15000) {
  const query = useQuery({
    queryKey: ["healthCheck"],
    queryFn: async () => {
      const response = await healthCheck();
      return {
        status: response.status,
        timestamp: response.timestamp,
        components: response.components ?? {},
      };
    },
    refetchInterval: refreshInterval,
    staleTime: 5000,
  });

  return {
    status: query.data?.status ?? "unknown",
    timestamp: query.data?.timestamp ?? "",
    components: query.data?.components ?? {},
    isLoading: query.isLoading,
    error: query.error ? parseBridgeError(query.error) : null,
    refetch: query.refetch,
  };
}
