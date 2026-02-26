/**
 * Intent Pool Contract Watcher
 *
 * Watches IntentCreated events on Intent Pool contracts to trigger real-time UI updates
 * Only watches the currently connected chain (cannot watch both chains simultaneously)
 */

import { useQueryClient } from '@tanstack/react-query'
import { useChainId, useWatchContractEvent } from 'wagmi'
import { INTENT_POOL_ABI, ETHEREUM_CONTRACTS, STARKNET_CONTRACTS } from '@/lib/contracts'

/**
 * Hook to watch IntentCreated events on the currently connected chain's Intent Pool
 * Invalidates React Query cache when new intents are created
 */
export function useIntentPoolWatch() {
  const queryClient = useQueryClient()
  const chainId = useChainId()

  // Determine which Intent Pool to watch based on connected chain
  const intentPoolAddress = chainId === 1
    ? ETHEREUM_CONTRACTS.intentPool
    : chainId === 2 // Mock ID for Starknet UI
    ? STARKNET_CONTRACTS.intentPool
    : undefined

  // Watch IntentCreated events on the current chain
  useWatchContractEvent({
    address: intentPoolAddress,
    abi: INTENT_POOL_ABI,
    eventName: 'IntentCreated',
    enabled: !!intentPoolAddress,
    onLogs(logs) {
      console.log('🔔 IntentCreated event detected:', logs.length, 'new intent(s)')

      // Invalidate bridge intents queries to trigger refetch
      queryClient.invalidateQueries({
        queryKey: ['bridgeIntents']
      })

      // Invalidate specific intent queries if needed
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      logs.forEach((log: any) => {
        const intentId = log.args.intentId
        if (intentId) {
          queryClient.invalidateQueries({
            queryKey: ['intentStatus', intentId]
          })
        }
      })
    },
    // Poll every block for new events
    pollingInterval: chainId === 2 ? 3000 : 12000, // Starknet ~3s, Ethereum ~12s
  })

  return {
    watchingChain: chainId === 1 ? 'ethereum' : chainId === 2 ? 'starknet' : 'unknown',
    isWatching: !!intentPoolAddress,
  }
}
