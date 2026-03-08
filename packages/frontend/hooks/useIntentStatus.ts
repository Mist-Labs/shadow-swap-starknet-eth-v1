import { useQuery } from "@tanstack/react-query"
import { getIntentStatus } from "@/lib/api"

interface UseIntentStatusParams {
    intentId: string | null
    enabled: boolean
    refetchInterval?: number
}

/**
 * Hook to fetch and track the status of a specific bridge intent.
 * Used by the BridgeProgress component to show real-time status updates.
 */
export function useIntentStatus({ intentId, enabled, refetchInterval = 5000 }: UseIntentStatusParams) {
    const { data, isLoading, error } = useQuery({
        queryKey: ["intent-status", intentId],
        queryFn: () => (intentId ? getIntentStatus(intentId) : null),
        enabled: enabled && !!intentId,
        refetchInterval: (query) => {
            // Stop polling if we reach a terminal state
            const status = query.state.data?.status
            if (status === "completed" || status === "failed" || status === "refunded" || status === "expired") {
                return false
            }
            return refetchInterval
        },
        retry: 3,
    })

    return {
        intentStatus: data?.status ?? null,
        fullIntent: data ?? null,
        isLoading,
        error: error instanceof Error ? error.message : null,
    }
}
