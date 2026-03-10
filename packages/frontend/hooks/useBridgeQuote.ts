import { useState, useCallback } from "react"
import { parseUnits } from "viem"
import { getAvnuQuote, type AvnuQuoteResult } from "@/lib/avnu"
import { createNearSwapQuote, type NearQuoteResponse } from "@/lib/near"
import { getTokenInfo, STRK_TOKEN, type ChainType } from "@/lib/tokens"
import { ETHEREUM_CONTRACTS, STARKNET_CONTRACTS } from "@/lib/contracts"
import { parseBridgeError, BridgeError } from "@/lib/errors"

interface UseBridgeQuoteParams {
    sourceChain: ChainType
    destChain: ChainType
    fromTokenSymbol: string
    toTokenSymbol: string
    amount: string
    walletAddress: string
}

export interface BridgeQuote {
    avnuQuote?: AvnuQuoteResult
    nearQuote: NearQuoteResponse
    effectiveAmount: string // amount to be sent to NEAR deposit address
}

export function useBridgeQuote() {
    const [isLoading, setIsLoading] = useState(false)
    const [error, setError] = useState<BridgeError | null>(null)
    const [quote, setQuote] = useState<BridgeQuote | null>(null)

    const fetchQuote = useCallback(async (params: UseBridgeQuoteParams) => {
        setIsLoading(true)
        setError(null)
        try {
            const tokenInfo = getTokenInfo(params.fromTokenSymbol, params.sourceChain)
            const destTokenInfo = getTokenInfo(params.toTokenSymbol, params.destChain)
            
            if (!tokenInfo) throw new Error(`Unsupported source token ${params.fromTokenSymbol}`)
            if (!destTokenInfo) throw new Error(`Unsupported destination token ${params.toTokenSymbol}`)

            const atomicAmount = parseUnits(params.amount, tokenInfo.decimals).toString()
            const needsMulticall = params.sourceChain === "starknet" && tokenInfo.requiresMulticall === true

            let avnuQuote: AvnuQuoteResult | undefined
            let bridgeAmount = atomicAmount

            // 1. Get AVNU quote if needed (Starknet non-STRK tokens)
            if (needsMulticall) {
                avnuQuote = await getAvnuQuote(
                    tokenInfo.address,
                    STRK_TOKEN.address,
                    atomicAmount,
                    params.walletAddress
                )
                bridgeAmount = avnuQuote.strkAmount
            }

            // 2. Get NEAR quote
            const originAsset = needsMulticall ? STRK_TOKEN.nearAssetId! : tokenInfo.nearAssetId!
            
            // CRITICAL:
            // - For Starknet Destination: NEAR ALWAYS delivers STRK (nep141:starknet.omft.near).
            //   If the user wants USDC/USDT, the relayer handles the swap via AVNU on Starknet.
            // - For EVM Destination:      NEAR delivers the specific asset requested (e.g., USDT/USDC).
            const destinationAsset = params.destChain === "starknet" 
                ? STRK_TOKEN.nearAssetId! 
                : (destTokenInfo.nearAssetId ?? `nep141:eth-${destTokenInfo.address.toLowerCase()}.omft.near`)

            const settlementContract = params.destChain === "ethereum" 
                ? ETHEREUM_CONTRACTS.intentPool 
                : STARKNET_CONTRACTS.intentPool

            const nearQuote = await createNearSwapQuote({
                originAsset,
                destinationAsset,
                amount: bridgeAmount,
                settlementContract,
                refundTo: params.walletAddress,
            })

            const result = { avnuQuote, nearQuote, effectiveAmount: bridgeAmount }
            setQuote(result)
            return result
        } catch (err: unknown) {
            const parsed = parseBridgeError(err)
            setError(parsed)
            throw parsed
        } finally {
            setIsLoading(false)
        }
    }, [])

    const reset = useCallback(() => {
        setQuote(null)
        setError(null)
    }, [])

    return { fetchQuote, quote, isLoading, error, reset }
}
