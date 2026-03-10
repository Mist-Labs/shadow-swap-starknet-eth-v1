import { useState, useCallback } from "react"
import { useBridgeQuote } from "./useBridgeQuote"
import { useBridgeExecution, type BridgeStep } from "./useBridgeExecution"
import type { AccountInterface } from "starknet"
import { type ChainType } from "@/lib/tokens"
import { BridgeError, parseBridgeError } from "@/lib/errors"

export type { BridgeStep }

interface BridgeParams {
    sourceChain: ChainType
    destChain: ChainType
    fromTokenSymbol: string
    toTokenSymbol: string
    amount: string
    recipient: string
    walletAddress: string
    starknetAccount?: AccountInterface
}

export function useBridge() {
    const { fetchQuote, quote, isLoading: isQuoteLoading, error: quoteError, reset: resetQuote } = useBridgeQuote()
    const { execute, step: executionStep, intentId, txHash, error: executionStepError, reset: resetExecution } = useBridgeExecution()

    const [internalError, setInternalError] = useState<BridgeError | null>(null)

    // Coordinate steps between hooks
    const step = useCallback(() => {
        if (executionStep !== "idle") return executionStep
        if (isQuoteLoading) return "fetching-quote"
        return "idle" as BridgeStep
    }, [executionStep, isQuoteLoading])()

    const error = internalError || executionStepError || quoteError

    const bridge = useCallback(async (params: BridgeParams) => {
        setInternalError(null) // Reset internal error on new bridge attempt
        try {
            // 1. Fetch Quote (AVNU + NEAR)
            const bridgeQuote = await fetchQuote({
                sourceChain: params.sourceChain,
                destChain: params.destChain,
                fromTokenSymbol: params.fromTokenSymbol,
                toTokenSymbol: params.toTokenSymbol,
                amount: params.amount,
                walletAddress: params.walletAddress,
            })

            // 2. Execute Transactions
            await execute({
                ...params,
                quote: bridgeQuote
            })

        } catch (err: unknown) {
            const parsed = parseBridgeError(err)
            setInternalError(parsed)
            throw parsed
        } finally {
            console.error("[useBridge] Bridge execution failed:", error)
        }
    }, [fetchQuote, execute, error])

    const reset = useCallback(() => {
        resetQuote()
        resetExecution()
        setInternalError(null)
    }, [resetQuote, resetExecution])

    return {
        bridge,
        reset,
        isLoading: isQuoteLoading || (executionStep !== "idle" && executionStep !== "completed" && executionStep !== "failed"),
        step,
        intentId,
        txHash,
        depositAddress: quote?.nearQuote.deposit_address || null,
        status: step,
        error,
    }
}
