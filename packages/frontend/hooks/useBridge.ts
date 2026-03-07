import { useState, useCallback } from "react"
import { parseUnits } from "viem"
import { sendTransaction, writeContract } from "wagmi/actions"
import { config as wagmiConfig } from "@/lib/web3/config"
import type { AccountInterface, Call } from "starknet"
import {
    generateEvmPrivacyParams,
    generateStarknetPrivacyParams,
    eciesEncryptBytes,
    eciesEncryptString,
    deriveViewKey,
} from "@/lib/crypto"
import { createNearSwapQuote, submitDepositToNear } from "@/lib/near"
import { getAvnuQuote, buildMulticall } from "@/lib/avnu"
import { initiateBridge } from "@/lib/api"
import { getTokenInfo, STRK_TOKEN, type ChainType } from "@/lib/tokens"
import { ETHEREUM_CONTRACTS, STARKNET_CONTRACTS } from "@/lib/contracts"

// ─── Step labels ──────

export type BridgeStep =
    | "idle"
    | "generating-params"
    | "fetching-quote"
    | "swapping-to-strk"   // AVNU multicall quote in progress (StarkNet ETH/USDT/USDC only)
    | "multicall-pending"  // waiting for StarkNet atomic tx confirmation
    | "creating-intent"
    | "submitting-backend"
    | "waiting-solver"
    | "completed"
    | "failed"

// ─── Types ────────────────────────────────────────────────────────────────────

interface BridgeParams {
    sourceChain: ChainType
    destChain: ChainType
    tokenSymbol: string
    amount: string          // human-readable decimal
    recipient: string       // destination address (EVM or StarkNet)
    walletAddress: string   // user's source-chain address
    /**
     * Connected StarkNet account from starknet-react useAccount().account.
     * Required when sourceChain === "starknet" and token requiresMulticall.
     */
    starknetAccount?: AccountInterface
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

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

            const tokenInfo = getTokenInfo(params.tokenSymbol, params.sourceChain)
            if (!tokenInfo) throw new Error(`Unsupported token ${params.tokenSymbol} on ${params.sourceChain}`)

            // needsMulticall: all non-STRK tokens on StarkNet require AVNU pre-swap
            const needsMulticall =
                params.sourceChain === "starknet" && tokenInfo.requiresMulticall === true

            // For multicall flows: relayer sees STRK (post-swap), never ETH/USDT/USDC.
            // For direct flows: relayer sees the user's actual token.
            const effectiveToken = needsMulticall ? STRK_TOKEN : tokenInfo
            const atomicAmount = parseUnits(params.amount, tokenInfo.decimals).toString()

            // ─── Step 1: Generate random intent ID ──────────────────────────────────────
            const intentIdBytes = crypto.getRandomValues(new Uint8Array(32))
            const backendIntentId =
                "0x" + Array.from(intentIdBytes).map((b) => b.toString(16).padStart(2, "0")).join("")
            setIntentId(backendIntentId)

            // ─── Step 2: Optionally get AVNU quote (multicall only) ──────────────────────
            let bridgeAmount = atomicAmount  // amount NEAR will actually receive
            let routeCalldata: string[] | null = null

            if (needsMulticall) {
                setStep("swapping-to-strk")

                if (!params.starknetAccount) {
                    throw new Error(
                        "StarkNet wallet not connected. Please connect your StarkNet wallet to bridge this token."
                    )
                }

                // Get AVNU quote: sellToken (ETH/USDT/USDC) → STRK
                // Pass slippageTolerance per spec (100 = 1%)
                const avnuQuote = await getAvnuQuote(
                    tokenInfo.address,
                    STRK_TOKEN.address,
                    atomicAmount,
                    params.walletAddress
                )

                bridgeAmount = avnuQuote.strkAmount
                routeCalldata = avnuQuote.routeCalldata
            }

            // ─── Step 3: Get NEAR quote ──────────────────────────────────────────────────
            setStep("fetching-quote")

            const originAsset = needsMulticall
                ? STRK_TOKEN.nearAssetId!          // NEAR receives STRK post-swap
                : tokenInfo.nearAssetId            // direct token's NEAR asset ID

            if (!originAsset) {
                throw new Error(
                    `No NEAR asset configured for ${params.tokenSymbol} on ${params.sourceChain}.`
                )
            }

            // destinationAsset: what NEAR delivers on the destination chain
            const destinationAsset = (() => {
                if (params.destChain === "ethereum") {
                    // Delivering to Ethereum — use the dest token's NEAR asset ID
                    return effectiveToken.nearAssetId ??
                        `nep141:eth-${effectiveToken.address.toLowerCase()}.omft.near`
                } else {
                    // Delivering to StarkNet — always STRK (only STRK is supported)
                    return STRK_TOKEN.nearAssetId!
                }
            })()

            const settlementContract =
                params.destChain === "ethereum"
                    ? ETHEREUM_CONTRACTS.intentPool
                    : STARKNET_CONTRACTS.intentPool

            const nearQuote = await createNearSwapQuote({
                originAsset,
                destinationAsset,
                amount: bridgeAmount,  // post-swap STRK for multicall, original amount for direct
                settlementContract,
                refundTo: params.walletAddress,
            })

            setDepositAddress(nearQuote.deposit_address)

            // ─── Step 4: Generate privacy params ────────────────────────────────────────
            setStep("creating-intent")

            const privacyParamsFn =
                params.sourceChain === "ethereum"
                    ? generateEvmPrivacyParams
                    : generateStarknetPrivacyParams

            const privacyData = privacyParamsFn({
                amount: bridgeAmount,
                token: effectiveToken.address,
                destChain: params.destChain === "ethereum" ? "evm" : "starknet",
            })

            // ─── Step 5: Encrypt sensitive fields ───────────────────────────────────────
            const encrypted_secret = eciesEncryptBytes(privacyData._secret)
            const encrypted_nullifier = eciesEncryptBytes(privacyData._nullifier)
            const encrypted_recipient = eciesEncryptString(params.recipient.toLowerCase())
            const viewKey = deriveViewKey(params.walletAddress, params.sourceChain)

            // ─── Step 6: Submit intent to relayer backend ────────────────────────────────
            setStep("submitting-backend")
            const initPayload = {
                intent_id: backendIntentId,
                source_chain: (params.sourceChain === "ethereum" ? "evm" : "starknet") as "evm" | "starknet",
                dest_chain: (params.destChain === "ethereum" ? "evm" : "starknet") as "evm" | "starknet",
                token: effectiveToken.address,     // STRK for multicall, original token otherwise
                amount: bridgeAmount,
                commitment: privacyData.commitment,
                nullifier_hash: privacyData.nullifier_hash,
                view_key: viewKey,
                near_intents_id: nearQuote.near_intents_id,
                encrypted_recipient,
                encrypted_secret,
                encrypted_nullifier,
                deposit_address: nearQuote.deposit_address,
            }

            const response = await initiateBridge(initPayload)
            if (!response.success) {
                throw new Error(response.message || "Failed to initiate on backend")
            }

            // ─── Step 7a: Multicall path (StarkNet ETH/USDT/USDC) ───────────────────────
            // Build multicall NOW (after NEAR quote) so we have the real deposit address.
            if (needsMulticall && routeCalldata && params.starknetAccount) {
                const finalCalls: Call[] = buildMulticall(
                    tokenInfo.address,
                    STRK_TOKEN.address,
                    atomicAmount,
                    bridgeAmount,
                    routeCalldata,
                    nearQuote.deposit_address  // real deposit address — not a placeholder
                )

                setStep("multicall-pending")
                const tx = await params.starknetAccount.execute(finalCalls)
                await params.starknetAccount.waitForTransaction(tx.transaction_hash)

                setTxHash(tx.transaction_hash)
                // Speed up NEAR indexing — non-fatal
                submitDepositToNear(tx.transaction_hash, nearQuote.deposit_address).catch(console.warn)
            }

            // ─── Step 7b: Direct path — user sends tokens to NEAR deposit address ────────
            // Source: Ethereum  → user transfers ERC-20 (or ETH) to nearQuote.deposit_address
            // Source: StarkNet STRK → user transfers STRK to nearQuote.deposit_address
            if (!needsMulticall) {
                setStep("multicall-pending") // re-use pending step for "waiting for wallet"

                if (params.sourceChain === "ethereum") {
                    // EVM direct send: transfer token to NEAR deposit address
                    if (tokenInfo.address === "0x0000000000000000000000000000000000000000") {
                        // Native ETH send
                        const hash = await sendTransaction(wagmiConfig, {
                            to: nearQuote.deposit_address as `0x${string}`,
                            value: BigInt(bridgeAmount),
                        })
                        setTxHash(hash)
                        submitDepositToNear(hash, nearQuote.deposit_address).catch(console.warn)
                    } else {
                        // ERC-20 transfer
                        const hash = await writeContract(wagmiConfig, {
                            address: tokenInfo.address as `0x${string}`,
                            abi: ERC20_TRANSFER_ABI,
                            functionName: "transfer",
                            args: [nearQuote.deposit_address as `0x${string}`, BigInt(bridgeAmount)],
                        })
                        setTxHash(hash)
                        submitDepositToNear(hash, nearQuote.deposit_address).catch(console.warn)
                    }
                } else {
                    // StarkNet STRK direct send: transfer STRK to NEAR deposit address
                    if (!params.starknetAccount) {
                        throw new Error("StarkNet wallet not connected.")
                    }
                    const tx = await params.starknetAccount.execute([{
                        contractAddress: STRK_TOKEN.address,
                        entrypoint: "transfer",
                        calldata: [nearQuote.deposit_address, bridgeAmount, "0"], // recipient, low, high
                    }])
                    await params.starknetAccount.waitForTransaction(tx.transaction_hash)
                    setTxHash(tx.transaction_hash)
                    submitDepositToNear(tx.transaction_hash, nearQuote.deposit_address).catch(console.warn)
                }
            }

            setStep("waiting-solver")
            console.log("✅ Intent created:", response)
        } catch (err: unknown) {
            console.error("Bridge Error:", err)
            setError(err instanceof Error ? err.message : "Bridge process failed")
            setStep("failed")
            throw err
        }
    }, [])

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
        isLoading: step !== "idle" && step !== "completed" && step !== "failed",
        step,
        intentId,
        txHash,
        depositAddress,
        status: step,
        error,
    }
}

// Minimal ABI for ERC-20 transfer
const ERC20_TRANSFER_ABI = [
    {
        name: "transfer",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [
            { name: "to", type: "address" },
            { name: "amount", type: "uint256" },
        ],
        outputs: [{ name: "", type: "bool" }],
    },
] as const
