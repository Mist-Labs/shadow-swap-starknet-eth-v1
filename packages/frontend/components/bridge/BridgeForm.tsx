"use client"
"use no memo"

import { useState, useEffect } from "react"
import Image from "next/image"
import { motion, AnimatePresence } from "framer-motion"
import { useAccount as useEvmAccount, useBalance } from "wagmi"
import { useAccount as useStarknetAccount } from "@starknet-react/core"
import { formatEther } from "viem"
import { toast } from "sonner"
import { Card } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ArrowDownUp, ArrowRight, Shield, Clock, DollarSign, AlertCircle, CheckCircle2, Wallet } from "lucide-react"
import BridgeProgress from "./BridgeProgress"
import { useBridge } from "@/hooks/useBridge"
import { useIntentStatus } from "@/hooks/useIntentStatus"
import { SUPPORTED_TOKENS, getTokenInfo } from "@/lib/tokens"

// Supported networks
const NETWORKS = [
    { id: "starknet", name: "Starknet", symbol: "STRK", logo: "S", logoPath: "/Starknet-logo.png", chain: "starknet" as const },
    { id: "ethereum", name: "Ethereum", symbol: "ETH", logo: "Ξ", logoPath: "/ethereum_logo.png", chain: "ethereum" as const },
]

export default function BridgeForm() {
    // EVM wallet (Reown / wagmi)
    const { address: evmAddress, isConnected: evmConnected } = useEvmAccount()
    // Starknet wallet (starknet-react)
    const { address: starknetAddress, isConnected: starknetConnected } = useStarknetAccount()

    // Form state — default FROM starknet (MVP only direction is live)
    const [fromNetwork, setFromNetwork] = useState(NETWORKS[0]) // Starknet
    const [toNetwork, setToNetwork] = useState(NETWORKS[1])     // Ethereum
    const [selectedToken, setSelectedToken] = useState<string>("ETH")
    const [amount, setAmount] = useState("")
    const [destinationAddress, setDestinationAddress] = useState("")
    const [useConnectedWallet, setUseConnectedWallet] = useState(true)
    const [showProgress, setShowProgress] = useState(false)
    const [modalClosing, setModalClosing] = useState(false)

    // Determine which wallet is needed & whether it's connected
    const isFromStarknet = fromNetwork.chain === "starknet"
    const activeWalletAddress = isFromStarknet ? starknetAddress : evmAddress
    const isRequiredWalletConnected = isFromStarknet ? starknetConnected : evmConnected

    // Bridge hook
    const { bridge, reset, isLoading, step, intentId, txHash, error } = useBridge()

    // Real-time intent status tracking
    const { intentStatus } = useIntentStatus({
        intentId: intentId,
        enabled: !!intentId && showProgress && !modalClosing,
        refetchInterval: 5000,
    })

    // Show progress modal when bridge starts
    useEffect(() => {
        if (isLoading && !showProgress && !modalClosing) {
            // eslint-disable-next-line react-hooks/set-state-in-effect
            setShowProgress(true)
            setModalClosing(false)
        }
    }, [isLoading, showProgress, modalClosing])

    // Auto-close modal on success
    useEffect(() => {
        if (!showProgress || modalClosing) return
        if (step !== "waiting-solver" && status !== "completed") return
        if (status === "completed") return

        if (step === "waiting-solver") {
            // eslint-disable-next-line react-hooks/set-state-in-effect
            setModalClosing(true)
            toast.success("Transaction submitted!", { duration: 4000 })
            const timer = setTimeout(() => {
                setShowProgress(false)
                setModalClosing(false)
            }, 2000)
            return () => clearTimeout(timer)
        }
    }, [step, showProgress, modalClosing])

    // Handle errors
    useEffect(() => {
        if (!showProgress || modalClosing) return
        if (step !== "failed" || !error) return

        // eslint-disable-next-line react-hooks/set-state-in-effect
        setModalClosing(true)
        toast.error(error || "Transaction failed. Please try again.", { duration: 5000 })

        const timer = setTimeout(() => {
            setShowProgress(false)
            setModalClosing(false)
            reset()
        }, 3000)
        return () => clearTimeout(timer)
    }, [step, showProgress, error, reset, modalClosing])

    // EVM balance (only available when from EVM)
    const { data: balanceData } = useBalance({
        address: evmAddress,
        chainId: !isFromStarknet ? 1 : undefined,
    })

    const calculateFees = () => {
        if (!amount || isNaN(Number(amount))) return { fee: "0", total: "0", receive: "0" }
        const amountNum = Number(amount)
        const fee = amountNum * (0.2 / 100)
        return {
            fee: fee.toFixed(6),
            total: amount,
            receive: (amountNum - fee).toFixed(6),
        }
    }

    const fees = calculateFees()

    const handleSwapNetworks = () => {
        const temp = fromNetwork
        setFromNetwork(toNetwork)
        setToNetwork(temp)
    }

    const handleSetMax = () => {
        if (balanceData && !isFromStarknet) {
            const balance = formatEther(balanceData.value)
            setAmount(Math.max(0, Number(balance) - 0.001).toFixed(6))
        }
    }

    const handleBridge = async () => {
        if (!isRequiredWalletConnected) {
            toast.error(
                isFromStarknet
                    ? "Connect your Starknet wallet to bridge from Starknet"
                    : "Connect your EVM wallet to bridge from Ethereum"
            )
            return
        }

        if (!amount || Number(amount) <= 0) {
            toast.error("Please enter an amount")
            return
        }

        const destination = useConnectedWallet
            ? activeWalletAddress
            : destinationAddress

        if (!destination) {
            toast.error("Please enter a destination address")
            return
        }

        try {
            await bridge({
                sourceChain: fromNetwork.chain,
                destChain: toNetwork.chain,
                tokenSymbol: selectedToken,
                amount,
                recipient: destination,
                walletAddress: activeWalletAddress as string,
            })
            setAmount("")
            setDestinationAddress("")
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : "Bridge failed"
            toast.error(msg)
        }
    }

    // Wallet connection status banner
    const walletBanner = !isRequiredWalletConnected && (
        <div className="mb-4 flex items-center gap-3 rounded-lg border border-amber-500/20 bg-amber-500/10 p-3 text-sm text-amber-400">
            <Wallet className="h-4 w-4 shrink-0" />
            <span>
                {isFromStarknet
                    ? "Connect your Starknet wallet (Argent X or Braavos) to bridge from Starknet"
                    : "Connect your EVM wallet to bridge from Ethereum"}
            </span>
        </div>
    )

    return (
        <>
            <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.5 }}>
                <Card className="mx-auto max-w-2xl border-neutral-800 bg-neutral-900/50 p-6 backdrop-blur-sm">
                    {/* Header */}
                    <div className="mb-6">
                        <h2 className="mb-2 text-2xl font-bold text-white">Bridge Assets</h2>
                        <p className="text-sm text-neutral-400">Transfer tokens across chains privately and instantly</p>
                    </div>

                    {/* Wallet connection banner */}
                    {walletBanner}

                    {/* Network Selection */}
                    <div className="mb-6 space-y-4">
                        {/* From Network */}
                        <div>
                            <Label className="mb-2 text-neutral-300">From</Label>
                            <Select
                                value={fromNetwork.id}
                                onValueChange={(value) => setFromNetwork(NETWORKS.find((n) => n.id === value)!)}
                            >
                                <SelectTrigger className="h-20 border-neutral-700 bg-neutral-800 py-4 text-white">
                                    <SelectValue>
                                        <div className="flex items-center gap-3">
                                            <div className="flex h-8 w-8 items-center justify-center overflow-hidden rounded-full bg-neutral-800">
                                                <Image
                                                    src={fromNetwork.logoPath}
                                                    alt={fromNetwork.name}
                                                    width={32}
                                                    height={32}
                                                    className="h-8 w-8 rounded-full object-cover"
                                                />
                                            </div>
                                            <div className="text-left">
                                                <div className="font-medium text-white">{fromNetwork.name}</div>
                                                <div className="text-xs text-neutral-500">
                                                    {isFromStarknet
                                                        ? starknetAddress
                                                            ? `${starknetAddress.slice(0, 8)}…${starknetAddress.slice(-4)}`
                                                            : "Starknet wallet not connected"
                                                        : balanceData
                                                            ? `Balance: ${formatEther(balanceData.value).slice(0, 8)} ETH`
                                                            : "EVM wallet not connected"}
                                                </div>
                                            </div>
                                        </div>
                                    </SelectValue>
                                </SelectTrigger>
                                <SelectContent>
                                    {NETWORKS.map((network) => (
                                        <SelectItem key={network.id} value={network.id}>
                                            <div className="flex items-center gap-3">
                                                <div className="flex h-8 w-8 items-center justify-center overflow-hidden rounded-full bg-neutral-800">
                                                    <Image
                                                        src={network.logoPath}
                                                        alt={network.name}
                                                        width={32}
                                                        height={32}
                                                        className="h-8 w-8 rounded-full object-cover"
                                                    />
                                                </div>
                                                <span>{network.name}</span>
                                            </div>
                                        </SelectItem>
                                    ))}
                                </SelectContent>
                            </Select>
                        </div>

                        {/* Swap Button */}
                        <div className="relative z-10 -my-2 flex justify-center">
                            <button
                                onClick={handleSwapNetworks}
                                className="flex h-10 w-10 items-center justify-center rounded-full border border-neutral-700 bg-neutral-800 transition-all duration-300 hover:rotate-180 hover:border-orange-500"
                            >
                                <ArrowDownUp className="h-5 w-5 text-orange-500" />
                            </button>
                        </div>

                        {/* To Network */}
                        <div>
                            <Label className="mb-2 text-neutral-300">To</Label>
                            <Select
                                value={toNetwork.id}
                                onValueChange={(value) => setToNetwork(NETWORKS.find((n) => n.id === value)!)}
                            >
                                <SelectTrigger className="h-20 border-neutral-700 bg-neutral-800 py-4 text-white">
                                    <SelectValue>
                                        <div className="flex items-center gap-3">
                                            <div className="flex h-8 w-8 items-center justify-center overflow-hidden rounded-full bg-neutral-800">
                                                <Image
                                                    src={toNetwork.logoPath}
                                                    alt={toNetwork.name}
                                                    width={32}
                                                    height={32}
                                                    className="h-8 w-8 rounded-full object-cover"
                                                />
                                            </div>
                                            <div className="text-left">
                                                <div className="font-medium text-white">{toNetwork.name}</div>
                                                <div className="text-xs text-neutral-500">Destination chain</div>
                                            </div>
                                        </div>
                                    </SelectValue>
                                </SelectTrigger>
                                <SelectContent>
                                    {NETWORKS.filter((n) => n.id !== fromNetwork.id).map((network) => (
                                        <SelectItem key={network.id} value={network.id}>
                                            <div className="flex items-center gap-3">
                                                <div className="flex h-8 w-8 items-center justify-center overflow-hidden rounded-full bg-neutral-800">
                                                    <Image
                                                        src={network.logoPath}
                                                        alt={network.name}
                                                        width={32}
                                                        height={32}
                                                        className="h-8 w-8 rounded-full object-cover"
                                                    />
                                                </div>
                                                <span>{network.name}</span>
                                            </div>
                                        </SelectItem>
                                    ))}
                                </SelectContent>
                            </Select>
                        </div>
                    </div>

                    {/* Token Selection */}
                    <div className="mb-6">
                        <Label className="mb-2 text-neutral-300">Token</Label>
                        <Select value={selectedToken} onValueChange={setSelectedToken}>
                            <SelectTrigger className="h-16 border-neutral-700 bg-neutral-800 text-white">
                                <SelectValue>
                                    <div className="flex items-center gap-3">
                                        <div className="flex h-10 w-10 items-center justify-center overflow-hidden rounded-full bg-neutral-800">
                                            <Image
                                                src={getTokenInfo(selectedToken, fromNetwork.chain)?.logo || "/ethereum_logo.png"}
                                                alt={selectedToken}
                                                width={40}
                                                height={40}
                                                className="h-10 w-10 rounded-full object-cover"
                                            />
                                        </div>
                                        <div className="text-left">
                                            <div className="font-medium text-white">{selectedToken}</div>
                                            <div className="text-xs text-neutral-500">1:1 Bridge</div>
                                        </div>
                                    </div>
                                </SelectValue>
                            </SelectTrigger>
                            <SelectContent>
                                {SUPPORTED_TOKENS.map((token) => {
                                    const tokenInfo = getTokenInfo(token, fromNetwork.chain)
                                    return (
                                        <SelectItem key={token} value={token}>
                                            <div className="flex items-center gap-3">
                                                <div className="flex h-10 w-10 items-center justify-center overflow-hidden rounded-full bg-neutral-800">
                                                    <Image
                                                        src={tokenInfo?.logo || "/ethereum_logo.png"}
                                                        alt={token}
                                                        width={40}
                                                        height={40}
                                                        className="h-10 w-10 rounded-full object-cover"
                                                    />
                                                </div>
                                                <div className="text-left">
                                                    <div className="font-medium">{tokenInfo?.name || token}</div>
                                                    <div className="text-xs text-neutral-500">{token}</div>
                                                </div>
                                            </div>
                                        </SelectItem>
                                    )
                                })}
                            </SelectContent>
                        </Select>
                    </div>

                    {/* Amount Input */}
                    <div className="mb-6">
                        <div className="mb-2 flex items-center justify-between">
                            <Label className="text-neutral-300">Amount</Label>
                            {!isFromStarknet && (
                                <button
                                    onClick={handleSetMax}
                                    className="text-xs text-orange-500 transition-colors hover:text-orange-400"
                                >
                                    MAX
                                </button>
                            )}
                        </div>
                        <div className="relative">
                            <Input
                                type="number"
                                placeholder="0.0"
                                value={amount}
                                onChange={(e) => setAmount(e.target.value)}
                                className="h-16 border-neutral-700 bg-neutral-800 pr-20 text-2xl font-bold text-white"
                            />
                            <div className="absolute right-4 top-1/2 -translate-y-1/2 transform text-neutral-500">
                                {selectedToken}
                            </div>
                        </div>
                    </div>

                    {/* Destination Address */}
                    <div className="mb-6">
                        <Label className="mb-2 text-neutral-300">Destination Address</Label>
                        <div className="space-y-3">
                            <div className="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    id="useConnected"
                                    checked={useConnectedWallet}
                                    onChange={(e) => setUseConnectedWallet(e.target.checked)}
                                    className="h-4 w-4 rounded border-neutral-700 bg-neutral-800 text-orange-500"
                                />
                                <label htmlFor="useConnected" className="cursor-pointer text-sm text-neutral-400">
                                    Use connected wallet address
                                </label>
                            </div>
                            {!useConnectedWallet && (
                                <Input
                                    type="text"
                                    placeholder="0x..."
                                    value={destinationAddress}
                                    onChange={(e) => setDestinationAddress(e.target.value)}
                                    className="border-neutral-700 bg-neutral-800 text-white"
                                />
                            )}
                        </div>
                    </div>

                    {/* Transaction Summary */}
                    {amount && Number(amount) > 0 && (
                        <motion.div
                            initial={{ opacity: 0, height: 0 }}
                            animate={{ opacity: 1, height: "auto" }}
                            className="mb-6 space-y-3 rounded-lg border border-neutral-700 bg-neutral-800/50 p-4"
                        >
                            <div className="flex items-center justify-between text-sm">
                                <span className="text-neutral-400">You send</span>
                                <div className="text-right">
                                    <div className="font-medium text-white">{amount} {selectedToken}</div>
                                    <div className="text-xs text-neutral-500">≈ $--</div>
                                </div>
                            </div>
                            <div className="flex items-center justify-between text-sm">
                                <span className="flex items-center gap-1 text-neutral-400">
                                    <DollarSign className="h-3 w-3" />
                                    Fee (0.2%)
                                </span>
                                <span className="text-neutral-300">{fees.fee} {selectedToken}</span>
                            </div>
                            <div className="flex items-center justify-between border-t border-neutral-700 pt-3">
                                <span className="flex items-center gap-1 text-neutral-400">
                                    <CheckCircle2 className="h-4 w-4 text-green-500" />
                                    You receive
                                </span>
                                <div className="text-right">
                                    <div className="text-lg font-bold text-white">{fees.receive} {selectedToken}</div>
                                </div>
                            </div>
                            <div className="flex items-center justify-between text-xs text-neutral-500">
                                <span className="flex items-center gap-1">
                                    <Clock className="h-3 w-3" />
                                    Est. time
                                </span>
                                <span>1–5 minutes (NEAR indexer)</span>
                            </div>
                            <div className="flex items-center gap-2 rounded border border-orange-500/20 bg-orange-500/10 p-2 text-xs text-neutral-500">
                                <Shield className="h-4 w-4 text-orange-500" />
                                <span>Privacy-enhanced with zero-knowledge commitments</span>
                            </div>
                        </motion.div>
                    )}

                    {/* Bridge Button */}
                    <Button
                        onClick={handleBridge}
                        disabled={!isRequiredWalletConnected || !amount || Number(amount) <= 0 || (isLoading && step !== "waiting-solver")}
                        className="h-12 w-full bg-orange-500 text-lg font-semibold text-white shadow-lg shadow-orange-500/20 transition-all duration-300 hover:scale-105 hover:bg-orange-600 disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:scale-100"
                    >
                        {!isRequiredWalletConnected ? (
                            isFromStarknet ? "Connect Starknet Wallet" : "Connect EVM Wallet"
                        ) : (isLoading && step !== "waiting-solver") ? (
                            <>
                                {step === "generating-params" && "Generating privacy params..."}
                                {step === "signing-auth" && "Sign authorization..."}
                                {step === "approving-token" && "Approving token..."}
                                {step === "creating-intent" && "Creating intent..."}
                                {step === "submitting-backend" && "Submitting to backend..."}
                            </>
                        ) : (
                            <>
                                Bridge Now
                                <ArrowRight className="ml-2 h-5 w-5" />
                            </>
                        )}
                    </Button>

                    {/* Info Box */}
                    <div className="mt-4 flex items-start gap-2 rounded border border-neutral-700 bg-neutral-800/50 p-3 text-xs text-neutral-500">
                        <AlertCircle className="mt-0.5 h-4 w-4 flex-shrink-0" />
                        <p>
                            Bridge transactions are privacy-preserving. Funds will appear in your destination wallet
                            in 1–5 minutes due to NEAR indexer latency.
                        </p>
                    </div>
                </Card>
            </motion.div>

            {/* Progress Modal */}
            <AnimatePresence>
                {showProgress && !modalClosing && (
                    <BridgeProgress
                        onClose={() => {
                            setShowProgress(false)
                            setModalClosing(false)
                            reset()
                        }}
                        fromNetwork={fromNetwork.name}
                        toNetwork={toNetwork.name}
                        amount={amount}
                        token={selectedToken}
                        step={step}
                        intentId={intentId || undefined}
                        txHash={txHash || undefined}
                        status={intentStatus || undefined}
                        error={error || undefined}
                    />
                )}
            </AnimatePresence>
        </>
    )
}

