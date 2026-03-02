"use client"

import { useMemo } from "react"
import { formatDistanceToNow } from "date-fns"
import { motion } from "framer-motion"
import Link from "next/link"
import { Card } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    CheckCircle2,
    Clock,
    XCircle,
    ArrowRight,
    Loader2,
    ExternalLink,
    AlertCircle,
    RefreshCw,
} from "lucide-react"
import { useBridgeIntents, formatChainName } from "@/hooks/useBridgeIntents"
import { getTxUrl } from "@/lib/contracts"
import type { ChainType } from "@/lib/tokens"

export default function RecentActivity() {
    const { intents, isLoading, isError, isFetching, refetch } = useBridgeIntents({ limit: 5 })

    const pendingCount = useMemo(() => {
        const terminalStates = ["completed", "refunded", "failed"]
        return intents.filter((i) => !terminalStates.includes(i.status)).length
    }, [intents])



    const formatTimeAgo = (dateValue: string | number | undefined) => {
        if (!dateValue) return "–"
        try {
            const date =
                typeof dateValue === "number"
                    ? new Date(dateValue * 1000)
                    : new Date(dateValue)
            if (isNaN(date.getTime())) return "–"
            return formatDistanceToNow(date, { addSuffix: true })
        } catch {
            return "–"
        }
    }

    const getStatusIcon = (status: string) => {
        switch (status) {
            case "completed":
                return <CheckCircle2 className="h-4 w-4 text-green-500" />
            case "filled":
                return <CheckCircle2 className="h-4 w-4 text-blue-500" />
            case "committed":
            case "created":
                return <Loader2 className="h-4 w-4 animate-spin text-orange-500" />
            case "failed":
                return <XCircle className="h-4 w-4 text-red-500" />
            default:
                return <Clock className="h-4 w-4 text-neutral-500" />
        }
    }

    const getStatusBadge = (status: string) => {
        const variants: Record<string, { class: string; text: string }> = {
            completed: { class: "bg-green-500/10 text-green-500 border-green-500/20", text: "Completed" },
            filled: { class: "bg-blue-500/10 text-blue-500 border-blue-500/20", text: "Filled" },
            committed: { class: "bg-orange-500/10 text-orange-400 border-orange-500/20", text: "Committed" },
            created: { class: "bg-orange-500/10 text-orange-500 border-orange-500/20", text: "Created" },
            failed: { class: "bg-red-500/10 text-red-500 border-red-500/20", text: "Failed" },
            refunded: { class: "bg-neutral-500/10 text-neutral-400 border-neutral-500/20", text: "Refunded" },
        }
        const v = variants[status] ?? variants.created
        return (
            <Badge variant="outline" className={`text-xs ${v.class}`}>
                {v.text}
            </Badge>
        )
    }

    // ── Loading skeleton ──────────────────────────────────────────────────────
    if (isLoading) {
        return (
            <Card className="border-neutral-800 bg-neutral-900/50 p-6 backdrop-blur-sm">
                <div className="mb-4 flex items-center justify-between">
                    <div>
                        <div className="h-5 w-32 animate-pulse rounded bg-neutral-800" />
                        <div className="mt-1 h-3 w-24 animate-pulse rounded bg-neutral-800" />
                    </div>
                </div>
                <div className="space-y-3">
                    {[0, 1, 2].map((i) => (
                        <div key={i} className="h-16 animate-pulse rounded-lg bg-neutral-800/50" />
                    ))}
                </div>
            </Card>
        )
    }

    // ── Error state ───────────────────────────────────────────────────────────
    if (isError) {
        return (
            <Card className="border-neutral-800 bg-neutral-900/50 p-6 backdrop-blur-sm">
                <div className="flex flex-col items-center gap-3 py-4 text-center">
                    <AlertCircle className="h-8 w-8 text-red-500/70" />
                    <p className="text-sm text-neutral-400">Could not load recent activity</p>
                    <Button
                        size="sm"
                        variant="outline"
                        onClick={() => refetch()}
                        className="border-neutral-700 bg-neutral-800 text-xs hover:bg-neutral-700"
                    >
                        <RefreshCw className="mr-1.5 h-3 w-3" />
                        Retry
                    </Button>
                </div>
            </Card>
        )
    }

    // ── Empty state: hide entirely ────────────────────────────────────────────
    if (intents.length === 0) {
        return null
    }

    // ── Data ──────────────────────────────────────────────────────────────────
    return (
        <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.5, delay: 0.1 }}
        >
            <Card className="border-neutral-800 bg-neutral-900/50 p-6 backdrop-blur-sm">
                {/* Header */}
                <div className="mb-4 flex items-center justify-between">
                    <div>
                        <div className="flex items-center gap-2">
                            <h3 className="text-lg font-bold text-white">Recent Activity</h3>
                            {isFetching && !isLoading && (
                                <Loader2 className="h-3.5 w-3.5 animate-spin text-neutral-500" />
                            )}
                        </div>
                        <p className="text-sm text-neutral-400">
                            {pendingCount > 0
                                ? `${pendingCount} pending transaction${pendingCount > 1 ? "s" : ""}`
                                : "All transactions settled"}
                        </p>
                    </div>
                    <Link href="/activity">
                        <Button
                            size="sm"
                            variant="outline"
                            className="border-neutral-700 bg-neutral-800 text-xs hover:bg-neutral-700"
                        >
                            View All
                            <ExternalLink className="ml-1 h-3 w-3" />
                        </Button>
                    </Link>
                </div>

                {/* Pending alert */}
                {pendingCount > 0 && (
                    <motion.div
                        initial={{ opacity: 0, scale: 0.95 }}
                        animate={{ opacity: 1, scale: 1 }}
                        className="mb-4 flex items-start gap-2 rounded-lg border border-orange-500/20 bg-orange-500/10 p-3"
                    >
                        <AlertCircle className="h-4 w-4 flex-shrink-0 text-orange-500" />
                        <div className="flex-1">
                            <p className="text-xs font-medium text-orange-400">
                                {pendingCount} transaction{pendingCount > 1 ? "s" : ""} in progress
                            </p>
                            <p className="mt-0.5 text-xs text-neutral-400">Auto-updating every 15s…</p>
                        </div>
                    </motion.div>
                )}

                {/* List */}
                <div className="space-y-3">
                    {intents.filter((i) => i.intent_id).map((intent, idx) => (
                        <motion.div
                            key={intent.intent_id}
                            initial={{ opacity: 0, x: -10 }}
                            animate={{ opacity: 1, x: 0 }}
                            transition={{ delay: idx * 0.05 }}
                            className="group relative overflow-hidden rounded-lg border border-neutral-700/50 bg-neutral-800/30 p-3 transition-all hover:border-orange-500/30 hover:bg-neutral-800/50"
                        >
                            {/* Status bar */}
                            <div
                                className={`absolute left-0 top-0 h-full w-1 ${intent.status === "completed"
                                    ? "bg-green-500"
                                    : intent.status === "failed"
                                        ? "bg-red-500"
                                        : "bg-orange-500"
                                    }`}
                            />

                            <div className="flex items-center justify-between">
                                <div className="flex items-center gap-3">
                                    {getStatusIcon(intent.status)}
                                    <div>
                                        <div className="flex items-center gap-2">
                                            <span className="text-sm font-medium text-white">
                                                {intent.amount} {intent.source_token}
                                            </span>
                                            {getStatusBadge(intent.status)}
                                        </div>
                                        <div className="mt-0.5 flex items-center gap-1.5 text-xs text-neutral-500">
                                            <span>{formatChainName(intent.source_chain)}</span>
                                            <ArrowRight className="h-3 w-3" />
                                            <span>{formatChainName(intent.dest_chain)}</span>
                                            <span>•</span>
                                            <span>{formatTimeAgo(intent.created_at)}</span>
                                        </div>
                                    </div>
                                </div>

                                {intent.source_settle_tx_hash && (
                                    <a
                                        href={getTxUrl(intent.source_chain as ChainType, intent.source_settle_tx_hash)}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        onClick={(e) => e.stopPropagation()}
                                        className="opacity-0 transition-opacity group-hover:opacity-100"
                                    >
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            className="h-7 px-2 text-neutral-400 hover:text-orange-500"
                                        >
                                            <ExternalLink className="h-3.5 w-3.5" />
                                        </Button>
                                    </a>
                                )}
                            </div>

                            <div className="mt-2 font-mono text-xs text-neutral-600">
                                {intent.intent_id.slice(0, 6)}…{intent.intent_id.slice(-4)}
                            </div>
                        </motion.div>
                    ))}
                </div>

                {intents.length >= 5 && (
                    <div className="mt-4 text-center">
                        <Link href="/activity">
                            <Button variant="ghost" size="sm" className="text-xs text-neutral-400 hover:text-orange-500">
                                View all transactions
                            </Button>
                        </Link>
                    </div>
                )}
            </Card>
        </motion.div>
    )
}
