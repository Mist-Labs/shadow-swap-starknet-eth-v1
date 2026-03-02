import { useState, useCallback } from "react"
import { parseUnits } from "viem"
import {
    generateEvmPrivacyParams,
    generateStarknetPrivacyParams,
    eciesEncryptBytes,
    eciesEncryptString,
    deriveViewKey
} from "@/lib/crypto"
import { createNearSwapQuote, submitDepositToNear } from "@/lib/near"
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
    amount: string // decimal string
    recipient: string
    walletAddress: string // EVM address or Starknet felt string
}

export function useBridge() {
    const [step, setStep] = useState<BridgeStep>("idle")
    const [intentId, setIntentId] = useState<string | null>(null)
    const [txHash, setTxHash] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [depositAddress, setDepositAddress] = useState<string | null>(null)

    const bridge = useCallback(async (params: BridgeParams) => {
        try {
            setStep("generating-params")
            setError(null)

            // 1. Generate Privacy Parameters (5-parameter commitment per spec §1)
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

            // 2. Fetch NEAR Quote (spec §4 — /v0/quote endpoint, read correlationId)
            setStep("fetching-quote")
            const originAsset = params.sourceChain === "ethereum"
                ? `nep141:eth-${tokenInfo.address}.omft.near`
                : `nep141:starknet.omft.near`

            const destinationAsset = params.destChain === "ethereum"
                ? `nep141:eth-${getTokenInfo(params.tokenSymbol, params.destChain)?.address}.omft.near`
                : `nep141:starknet.omft.near`

            // Spec §1 NEAR Recipient Rule: MUST be the settlement contract, not the end user
            const settlementContract = params.destChain === "ethereum"
                ? process.env.NEXT_PUBLIC_EVM_SETTLEMENT!
                : process.env.NEXT_PUBLIC_STARKNET_SETTLEMENT!

            const nearQuote = await createNearSwapQuote({
                originAsset,
                destinationAsset,
                amount: atomicAmount,
                settlementContract,
                refundTo: params.walletAddress
            })

            setDepositAddress(nearQuote.deposit_address)

            // 3. ECIES Encrypt (spec §2)
            setStep("creating-intent")
            const encrypted_secret = eciesEncryptBytes(privacyData._secret)
            const encrypted_nullifier = eciesEncryptBytes(privacyData._nullifier)
            // Recipient MUST be UTF-8 encoded (spec §2 — NOT hex bytes)
            const encrypted_recipient = eciesEncryptString(params.recipient.toLowerCase())
            const viewKey = deriveViewKey(params.walletAddress, params.sourceChain)

            // 4. Generate intent ID using crypto.getRandomValues (spec security checklist)
            const intentIdBytes = crypto.getRandomValues(new Uint8Array(32))
            const backendIntentId = "0x" + Array.from(intentIdBytes)
                .map(b => b.toString(16).padStart(2, "0")).join("")
            setIntentId(backendIntentId)

            // 5. Submit to Relayer Backend (via /api/bridge server-side proxy)
            setStep("submitting-backend")
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
                claim_auth: "0x0",
                encrypted_recipient,
                refund_address: params.walletAddress,
                near_intents_id: nearQuote.near_intents_id, // spec §3: correlationId UUID
                view_key: viewKey,
                deposit_address: nearQuote.deposit_address
            }

            const response = await initiateBridge(initPayload)
            if (!response.success) {
                throw new Error(response.message || "Failed to initiate on backend")
            }

            // 6. The wallet now sends tokens to deposit_address.
            //    After source-chain tx confirms, call onSourceTxConfirmed(txHash)
            //    which submits the hash to NEAR 1Click (spec §5, non-fatal).
            setStep("waiting-solver")
            setTxHash(null) // Set via onSourceTxConfirmed after wallet signing

            console.log("✅ Intent created:", response)

        } catch (err: unknown) {
            console.error("Bridge Error:", err)
            const msg = err instanceof Error ? err.message : "Bridge process failed"
            setError(msg)
            setStep("failed")
            throw err
        }
    }, [])

    /**
     * Call after the user's wallet tx is confirmed on the source chain.
     * Submits the tx hash to NEAR 1Click to reduce indexer latency (spec §5).
     * Non-fatal: NEAR will detect the deposit automatically if this fails.
     */
    const onSourceTxConfirmed = useCallback(async (confirmedTxHash: string) => {
        setTxHash(confirmedTxHash)
        if (depositAddress) {
            submitDepositToNear(confirmedTxHash, depositAddress).catch(console.warn)
        }
    }, [depositAddress])

    const reset = useCallback(() => {
        setStep("idle")
        setIntentId(null)
        setTxHash(null)
        setDepositAddress(null)
        setError(null)
    }, [])

    return {
        bridge,
        reset,
        onSourceTxConfirmed,
        isLoading: step !== "idle" && step !== "completed" && step !== "failed",
        step,
        intentId,
        txHash,
        depositAddress,
        status: step,
        error,
    }
}
