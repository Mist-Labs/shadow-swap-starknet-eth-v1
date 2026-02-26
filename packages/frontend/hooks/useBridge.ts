import { useState, useCallback } from "react"
import { type Address, parseUnits } from "viem"
import {
    generateEvmPrivacyParams,
    generateStarknetPrivacyParams,
    eciesEncryptBytes,
    eciesEncryptString,
    deriveViewKey
} from "@/lib/crypto"
import { createNearSwapQuote } from "@/lib/near"
import { initiateBridge } from "@/lib/api"
import { getTokenInfo, type ChainType } from "@/lib/tokens"

export type BridgeStep =
    | "idle"
    | "generating-params"
    | "fetching-quote"
    | "signing-auth"
    | "approving-token"
    | "creating-intent"
    | "submitting-backend"
    | "waiting-solver"
    | "completed"
    | "failed"

interface BridgeParams {
    sourceChain: ChainType
    destChain: ChainType
    tokenSymbol: string
    amount: string // as a string decimal (needs conversion depending on token)
    recipient: Address
    walletAddress: Address // Needed for viewKey
}

export function useBridge() {
    const [step, setStep] = useState<BridgeStep>("idle")
    const [intentId, setIntentId] = useState<string | null>(null)
    const [txHash, setTxHash] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)

    const bridge = useCallback(async (params: BridgeParams) => {
        try {
            setStep("generating-params")
            setError(null)
            
            // 1. Generate Privacy Parameters
            const tokenInfo = getTokenInfo(params.tokenSymbol, params.sourceChain)
            if (!tokenInfo) throw new Error("Unsupported token on source chain")

            const atomicAmount = parseUnits(params.amount, tokenInfo.decimals).toString()

            const privacyParamsFn = params.sourceChain === "ethereum" 
                ? generateEvmPrivacyParams 
                : generateStarknetPrivacyParams

            const privacyData = privacyParamsFn({
                amount: atomicAmount,
                token: tokenInfo.address,
                destChain: params.destChain
            })

            // 2. Fetch NEAR Quote
            setStep("fetching-quote")
            const originAsset = params.sourceChain === "ethereum" 
                ? `nep141:eth-${tokenInfo.address}.omft.near`
                : `nep141:starknet.omft.near` // simplifying for STRK vs USDT StarkNet routing etc.
            
            const destinationAsset = params.destChain === "ethereum"
                ? `nep141:eth-${getTokenInfo(params.tokenSymbol, params.destChain)?.address}.omft.near`
                : `nep141:starknet.omft.near`
            
            const settlementContract = params.destChain === "ethereum"
                ? process.env.NEXT_PUBLIC_EVM_SETTLEMENT!
                : process.env.NEXT_PUBLIC_STARKNET_SETTLEMENT!

            const nearQuote = await createNearSwapQuote({
                originAsset,
                destinationAsset,
                amount: atomicAmount,
                settlementContract, // MUST BE CONTRACT, NOT USER
                refundTo: params.walletAddress
            })

            // 3. ECIES Encrypt sensitive payload
            setStep("creating-intent")
            
            const encrypted_secret = eciesEncryptBytes(privacyData._secret)
            const encrypted_nullifier = eciesEncryptBytes(privacyData._nullifier)
            const encrypted_recipient = eciesEncryptString(params.recipient.toLowerCase()) // UTF8 STRING

            const viewKey = deriveViewKey(params.walletAddress, params.sourceChain)
            
            // 4. Submit to Backend
            setStep("submitting-backend")
            const backendIntentId = "0x" + Array.from(crypto.getRandomValues(new Uint8Array(32)))
                .map(b => b.toString(16).padStart(2, "0")).join("")

            setIntentId(backendIntentId)

            const initPayload = {
                intent_id: backendIntentId,
                user_address: params.walletAddress,
                source_chain: params.sourceChain,
                dest_chain: params.destChain,
                source_token: tokenInfo.address,
                dest_token: getTokenInfo(params.tokenSymbol, params.destChain)!.address,
                amount: atomicAmount,
                commitment: privacyData.commitment,
                encrypted_secret,
                encrypted_nullifier,
                nullifier_hash: privacyData.nullifier_hash,
                claim_auth: "0x0", // mock claim auth wrapper for now
                encrypted_recipient,
                refund_address: params.walletAddress,
                near_intents_id: nearQuote.near_intents_id,
                view_key: viewKey,
                deposit_address: nearQuote.deposit_address
            }

            const response = await initiateBridge(initPayload)
            
            if (!response.success) {
                throw new Error(response.message || "Failed to initiate on backend")
            }

            // Move to waiting for solver mapping and on-chain
            setStep("waiting-solver")
            // Normally we'd send transaction to wallet here (paying depositAddress on near)
            // setTxHash(response.txid)
            setTxHash("0x" + Math.random().toString(16).slice(2).padStart(64, "0")) 
            
            console.log("✅ Intent successfully created:", response)
            
        } catch (err: unknown) {
            console.error("Bridge Error:", err)
            const msg = err instanceof Error ? err.message : "Bridge process failed"
            setError(msg)
            setStep("failed")
            throw err
        }
    }, [])

    const reset = useCallback(() => {
        setStep("idle")
        setIntentId(null)
        setTxHash(null)
        setError(null)
    }, [])

    return {
        bridge,
        reset,
        isLoading: step !== "idle" && step !== "completed" && step !== "failed",
        step,
        intentId,
        txHash,
        status: step, 
        error,
    }
}
