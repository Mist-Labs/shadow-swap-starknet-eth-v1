/**
 * useBridgeStats Hook
 *
 * Hook for fetching bridge statistics and health data from the backend
 */

import { useState, useEffect, useCallback } from "react";
import { getBridgeStats, healthCheck } from "@/lib/api";

/**
 * Bridge statistics state
 */
interface BridgeStatsState {
  totalIntents: number;
  pendingIntents: number;
  filledIntents: number;
  completedIntents: number;
  failedIntents: number;
  refundedIntents: number;
  ethereumToStarknet: number;
  starknetToEthereum: number;
  volumeByToken: Record<string, string>;
  isLoading: boolean;
  error: string | null;
}

/**
 * Health check state
 */
interface HealthCheckState {
  status: string;
  timestamp: string;
  components: Record<string, string>;
  isLoading: boolean;
  error: string | null;
}

/**
 * Hook for fetching bridge statistics
 */
export function useBridgeStats(refreshInterval: number = 30000) {
  const [state, setState] = useState<BridgeStatsState>({
    totalIntents: 0,
    pendingIntents: 0,
    filledIntents: 0,
    completedIntents: 0,
    failedIntents: 0,
    refundedIntents: 0,
    ethereumToStarknet: 0,
    starknetToEthereum: 0,
    volumeByToken: {},
    isLoading: true,
    error: null,
  });

  const fetchStats = useCallback(async () => {
    try {
      setState((prev) => ({ ...prev, isLoading: true, error: null }));

      const response = await getBridgeStats();

      setState({
        totalIntents: response.data.total_intents,
        pendingIntents: response.data.pending_intents,
        filledIntents: response.data.filled_intents ?? response.data.settled_intents ?? 0,
        completedIntents: response.data.completed_intents ?? response.data.settled_intents ?? 0,
        failedIntents: response.data.failed_intents,
        refundedIntents: response.data.refunded_intents,
        ethereumToStarknet: response.data.ethereum_to_starknet ?? 0,
        starknetToEthereum: response.data.starknet_to_ethereum ?? 0,
        volumeByToken: response.data.total_volume_by_token ?? {},
        isLoading: false,
        error: null,
      });
    } catch (error) {
      console.error("Failed to fetch bridge stats:", error);
      setState((prev) => ({
        ...prev,
        isLoading: false,
        error: error instanceof Error ? error.message : "Failed to fetch stats",
      }));
    }
  }, []);

  // Initial fetch
  useEffect(() => {
    fetchStats();
  }, [fetchStats]);

  // Auto-refresh
  useEffect(() => {
    if (refreshInterval > 0) {
      const interval = setInterval(fetchStats, refreshInterval);
      return () => clearInterval(interval);
    }
  }, [refreshInterval, fetchStats]);

  return {
    ...state,
    refetch: fetchStats,
  };
}

/**
 * Hook for health check
 */
export function useHealthCheck(refreshInterval: number = 15000) {
  const [state, setState] = useState<HealthCheckState>({
    status: "unknown",
    timestamp: "",
    components: {},
    isLoading: true,
    error: null,
  });

  const fetchHealth = useCallback(async () => {
    try {
      setState((prev) => ({ ...prev, isLoading: true, error: null }));

      const response = await healthCheck();

      setState({
        status: response.status,
        timestamp: response.timestamp,
        components: response.components ?? {},
        isLoading: false,
        error: null,
      });
    } catch (error) {
      console.error("Failed to fetch health check:", error);
      setState({
        status: "error",
        timestamp: new Date().toISOString(),
        components: {},
        isLoading: false,
        error: error instanceof Error ? error.message : "Failed to fetch health",
      });
    }
  }, []);

  // Initial fetch
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchHealth();
  }, [fetchHealth]);

  // Auto-refresh
  useEffect(() => {
    if (refreshInterval > 0) {
      const interval = setInterval(fetchHealth, refreshInterval);
      return () => clearInterval(interval);
    }
  }, [refreshInterval, fetchHealth]);

  return {
    ...state,
    refetch: fetchHealth,
  };
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
 * Format large numbers for display
 */
export function formatLargeNumber(num: number): string {
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(2)}M`;
  }
  if (num >= 1000) {
    return `${(num / 1000).toFixed(2)}K`;
  }
  return num.toString();
}
