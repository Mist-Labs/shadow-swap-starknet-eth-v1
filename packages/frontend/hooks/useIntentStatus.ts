import type { IntentStatus } from "@/lib/api"

interface UseIntentStatusParams {
  intentId: string | null
  enabled: boolean
  refetchInterval?: number
}

export function useIntentStatus({ enabled }: UseIntentStatusParams) {
  // Mock implementation
  const intentStatus: IntentStatus | null = enabled ? "processing" : null
  
  return {
    intentStatus,
    isLoading: false
  }
}
