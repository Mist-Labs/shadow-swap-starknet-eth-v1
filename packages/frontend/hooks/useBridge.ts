import { useState, useCallback } from "react"
import type { Address } from "viem"

export type BridgeStep =
  | "idle"
  | "generating-params"
  | "signing-auth"
  | "approving-token"
  | "creating-intent"
  | "submitting-backend"
  | "waiting-solver"
  | "completed"
  | "failed"

interface BridgeParams {
  sourceChain: "ethereum" | "starknet"
  destChain: "ethereum" | "starknet"
  tokenSymbol: string
  amount: string
  recipient: Address
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
      
      // Simulation of bridge steps
      console.log("Bridging with params:", params)
      
      await new Promise(r => setTimeout(r, 1000))
      setStep("creating-intent")
      
      await new Promise(r => setTimeout(r, 1000))
      setStep("submitting-backend")
      setIntentId("0x" + Math.random().toString(16).slice(2).padStart(64, "0"))
      
      await new Promise(r => setTimeout(r, 1000))
      setStep("waiting-solver")
      setTxHash("0x" + Math.random().toString(16).slice(2).padStart(64, "0"))
      
      // In a real implementation, we would poll here or waiting for WebSocket update
      
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } catch (err: any) {
      console.error(err)
      setError(err.message || "Bridge failed")
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
    status: step, // simplifying for now
    error,
  }
}
