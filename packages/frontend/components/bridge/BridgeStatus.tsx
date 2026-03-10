import React from "react"
import { Badge } from "@/components/ui/badge"
import { CheckCircle2, Clock, XCircle, Loader2, ArrowRight } from "lucide-react"

export type IntentStatus =
    | "processing"
    | "bridging"
    | "settling"
    | "completed"
    | "refunded"
    | "failed"
    | "expired"
    | "created"
    | "committed"
    | "filled"

interface BridgeStatusProps {
    status: string
    showIcon?: boolean
}

export const getStatusConfig = (status: string) => {
    const variants: Record<string, {
        class: string;
        text: string;
        icon: React.ReactNode;
        colorClass: string;
    }> = {
        completed: {
            class: "bg-green-500/10 text-green-500 border-green-500/20",
            text: "Completed",
            icon: <CheckCircle2 className="h-4 w-4 text-green-500" />,
            colorClass: "text-green-500"
        },
        filled: {
            class: "bg-blue-500/10 text-blue-500 border-blue-500/20",
            text: "Filled",
            icon: <CheckCircle2 className="h-4 w-4 text-blue-500" />,
            colorClass: "text-blue-500"
        },
        committed: {
            class: "bg-purple-500/10 text-purple-500 border-purple-500/20",
            text: "Committed",
            icon: <Loader2 className="h-4 w-4 animate-spin text-purple-500" />,
            colorClass: "text-purple-500"
        },
        created: {
            class: "bg-orange-500/10 text-orange-500 border-orange-500/20",
            text: "Created",
            icon: <Loader2 className="h-4 w-4 animate-spin text-orange-500" />,
            colorClass: "text-orange-500"
        },
        bridging: {
            class: "bg-orange-500/10 text-orange-500 border-orange-500/20",
            text: "Bridging",
            icon: <Loader2 className="h-4 w-4 animate-spin text-orange-500" />,
            colorClass: "text-orange-500"
        },
        processing: {
            class: "bg-orange-500/10 text-orange-500 border-orange-500/20",
            text: "Processing",
            icon: <Loader2 className="h-4 w-4 animate-spin text-orange-500" />,
            colorClass: "text-orange-500"
        },
        settling: {
            class: "bg-blue-500/10 text-blue-500 border-blue-500/20",
            text: "Settling",
            icon: <Loader2 className="h-4 w-4 animate-spin text-blue-500" />,
            colorClass: "text-blue-500"
        },
        refunded: {
            class: "bg-yellow-500/10 text-yellow-500 border-yellow-500/20",
            text: "Refunded",
            icon: <ArrowRight className="h-4 w-4 text-yellow-500" />,
            colorClass: "text-yellow-500"
        },
        failed: {
            class: "bg-red-500/10 text-red-500 border-red-500/20",
            text: "Failed",
            icon: <XCircle className="h-4 w-4 text-red-500" />,
            colorClass: "text-red-500"
        },
        expired: {
            class: "bg-neutral-500/10 text-neutral-400 border-neutral-500/20",
            text: "Expired",
            icon: <Clock className="h-4 w-4 text-neutral-500" />,
            colorClass: "text-neutral-500"
        },
    }

    return variants[status] || {
        class: "bg-neutral-500/10 text-neutral-500 border-neutral-500/20",
        text: status.charAt(0).toUpperCase() + status.slice(1),
        icon: <Clock className="h-4 w-4 text-neutral-500" />,
        colorClass: "text-neutral-500"
    }
}

export function BridgeStatusBadge({ status }: BridgeStatusProps) {
    const config = getStatusConfig(status)
    return (
        <Badge variant="outline" className={`text-xs ${config.class}`}>
            {config.text}
        </Badge>
    )
}

export function BridgeStatusIcon({ status }: BridgeStatusProps) {
    const config = getStatusConfig(status)
    return config.icon
}
