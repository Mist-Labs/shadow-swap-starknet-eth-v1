import { useState, useCallback } from "react"
import { parseUnits } from "viem"
import { sendTransaction, writeContract } from "wagmi/actions"
import { config as wagmiConfig } from "@/lib/web3/config"
import type { AccountInterface } from "starknet"
import { cairo } from "starknet"
import {
    generateEvmPrivacyParams,
    generateStarknetPrivacyParams,
    eciesEncryptBytes,
    eciesEncryptString,
    deriveViewKey,
} from "@/lib/crypto"
import { submitDepositToNear } from "@/lib/near"
import { getAvnuSwapCall, buildMulticall, AVNU_SLIPPAGE } from "@/lib/avnu"
import { initiateBridge } from "@/lib/api"
import { getTokenInfo, STRK_TOKEN, type ChainType } from "@/lib/tokens"
import { parseBridgeError, BridgeError } from "@/lib/errors"
import type { BridgeQuote } from "./useBridgeQuote"

export type BridgeStep =
    | "idle"
    | "generating-params"
    | "fetching-quote"
    | "swapping-to-strk"
    | "multicall-pending"
    | "creating-intent"
    | "submitting-backend"
    | "waiting-solver"
    | "completed"
    | "failed"

interface ExecutionParams {
    sourceChain: ChainType
    destChain: ChainType
    fromTokenSymbol: string
    toTokenSymbol: string
    amount: string // original amount
    recipient: string
    walletAddress: string
    starknetAccount?: AccountInterface
    quote: BridgeQuote
}

export function useBridgeExecution() {
    const [step, setStep] = useState<BridgeStep>("idle")
    const [intentId, setIntentId] = useState<string | null>(null)
    const [txHash, setTxHash] = useState<string | null>(null)
    const [error, setError] = useState<BridgeError | null>(null)

    const execute = useCallback(async (params: ExecutionParams) => {
        setStep("generating-params")
        setError(null)

        try {
            const tokenInfo = getTokenInfo(params.fromTokenSymbol, params.sourceChain)!
            const destTokenInfo = getTokenInfo(params.toTokenSymbol, params.destChain)!
            const needsMulticall = !!params.quote.avnuQuote
            const effectiveToken = needsMulticall ? STRK_TOKEN : tokenInfo

            // 1. Generate Intent ID / View Key
            const isDestStarknet = params.destChain === "starknet"
            const byteLen = isDestStarknet ? 31 : 32
            
            const intentIdBytes = crypto.getRandomValues(new Uint8Array(byteLen))
            const backendIntentId = "0x" + Array.from(intentIdBytes).map(b => b.toString(16).padStart(2, '0')).join('').padStart(64, '0')
            setIntentId(backendIntentId)

            // 2. Generate Privacy Params
            setStep("creating-intent")
            const privacyParamsFn = params.sourceChain === "ethereum" ? generateEvmPrivacyParams : generateStarknetPrivacyParams
            const privacyData = privacyParamsFn({
                amount: params.quote.effectiveAmount,
                token: effectiveToken.address,
                destChain: params.destChain === "ethereum" ? "evm" : "starknet"
            })

            // 3. Encrypt & View Key
            const encrypted_secret = eciesEncryptBytes(privacyData._secret)
            const encrypted_nullifier = eciesEncryptBytes(privacyData._nullifier)
            const encrypted_recipient = eciesEncryptString(params.recipient.toLowerCase())
            const viewKey = deriveViewKey(params.walletAddress, params.sourceChain, params.destChain)

            // 4. Submit to Backend
            setStep("submitting-backend")
            
            // CRITICAL:
            // - If dest is Starknet: dest_token = Starknet Contract Address (relayer swaps STRK -> dest_token via AVNU)
            // - If dest is EVM:      dest_token = OMITTED (NEAR delivers the target asset directly to EVM)
            const dest_token = params.destChain === "starknet" ? destTokenInfo.address : undefined

            const initPayload = {
                intent_id: backendIntentId,
                source_chain: (params.sourceChain === "ethereum" ? "evm" : "starknet") as "evm" | "starknet",
                dest_chain: (params.destChain === "ethereum" ? "evm" : "starknet") as "evm" | "starknet",
                token: effectiveToken.address,
                amount: params.quote.effectiveAmount,
                commitment: privacyData.commitment,
                nullifier_hash: privacyData.nullifier_hash,
                view_key: viewKey,
                near_intents_id: params.quote.nearQuote.near_intents_id,
                encrypted_recipient,
                encrypted_secret,
                encrypted_nullifier,
                deposit_address: params.quote.nearQuote.deposit_address,
                dest_token // Conditional based on destination chain requirements
            }

            const response = await initiateBridge(initPayload)
            if (!response.success) throw new Error(response.message || "Failed to initiate on backend")

            // 5. Chain Execution
            if (needsMulticall && params.starknetAccount) {
                setStep("swapping-to-strk")
                const swapCall = await getAvnuSwapCall(params.quote.avnuQuote!.quote, params.walletAddress, AVNU_SLIPPAGE)
                const finalCalls = buildMulticall(
                    tokenInfo.address,
                    STRK_TOKEN.address,
                    parseUnits(params.amount, tokenInfo.decimals).toString(),
                    params.quote.effectiveAmount,
                    swapCall,
                    params.quote.nearQuote.deposit_address
                )

                setStep("multicall-pending")
                const tx = await params.starknetAccount.execute(finalCalls)
                await params.starknetAccount.waitForTransaction(tx.transaction_hash)
                setTxHash(tx.transaction_hash)
                submitDepositToNear(tx.transaction_hash, params.quote.nearQuote.deposit_address).catch(console.warn)
            } else {
                setStep("multicall-pending")
                if (params.sourceChain === "ethereum") {
                    let hash: `0x${string}`
                    if (tokenInfo.address === "0x0000000000000000000000000000000000000000") {
                        hash = await sendTransaction(wagmiConfig, {
                            to: params.quote.nearQuote.deposit_address as `0x${string}`,
                            value: BigInt(params.quote.effectiveAmount)
                        })
                    } else {
                        hash = await writeContract(wagmiConfig, {
                            address: tokenInfo.address as `0x${string}`,
                            abi: ERC20_TRANSFER_ABI,
                            functionName: "transfer",
                            args: [params.quote.nearQuote.deposit_address as `0x${string}`, BigInt(params.quote.effectiveAmount)]
                        })
                    }
                    setTxHash(hash)
                    submitDepositToNear(hash, params.quote.nearQuote.deposit_address).catch(console.warn)
                } else {
                    if (!params.starknetAccount) throw new Error("StarkNet wallet not connected.")
                    const u256Amount = cairo.uint256(params.quote.effectiveAmount)
                    const tx = await params.starknetAccount.execute([{
                        contractAddress: STRK_TOKEN.address,
                        entrypoint: "transfer",
                        calldata: [params.quote.nearQuote.deposit_address, u256Amount.low, u256Amount.high].map(v => v.toString())
                    }])
                    await params.starknetAccount.waitForTransaction(tx.transaction_hash)
                    setTxHash(tx.transaction_hash)
                    submitDepositToNear(tx.transaction_hash, params.quote.nearQuote.deposit_address).catch(console.warn)
                }
            }

            setStep("waiting-solver")
        } catch (err: unknown) {
            const parsed = parseBridgeError(err)
            setError(parsed)
            setStep("failed")
            throw parsed
        }
    }, [])

    const reset = useCallback(() => {
        setStep("idle")
        setIntentId(null)
        setTxHash(null)
        setError(null)
    }, [])

    return { execute, step, intentId, txHash, error, reset }
}

const ERC20_TRANSFER_ABI = [
    {
        name: "transfer",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [{ name: "to", type: "address" }, { name: "amount", type: "uint256" }],
        outputs: [{ name: "", type: "bool" }]
    }
] as const
