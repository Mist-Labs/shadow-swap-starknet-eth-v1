"use client"

import * as React from "react"
import { useConnect } from "@starknet-react/core"
import { StarknetkitConnector, useStarknetkitConnectModal } from "starknetkit"
import { Button, type ButtonProps } from "@/components/ui/button"
import { Wallet } from "lucide-react"
import { cn } from "@/lib/utils"

interface StarknetConnectModalProps extends ButtonProps {
    buttonText?: string
}

const StarknetConnectModal = React.forwardRef<HTMLButtonElement, StarknetConnectModalProps>(
    ({ className, buttonText = "Connect Starknet Wallet", ...props }, ref) => {
        const { connectAsync, connectors } = useConnect()

        const { starknetkitConnectModal } = useStarknetkitConnectModal({
            connectors: connectors as StarknetkitConnector[],
            modalTheme: "dark",
        })

        const handleConnect = async (e: React.MouseEvent<HTMLButtonElement>) => {
            // Prevent event bubbling if needed
            // e.preventDefault()

            try {
                const { connector } = await starknetkitConnectModal()
                if (!connector) return
                await connectAsync({ connector })
            } catch (error) {
                console.error("Failed to connect Starknet wallet:", error)
            }

            // Call original onClick if provided
            if (props.onClick) props.onClick(e)
        }

        return (
            <Button
                ref={ref}
                {...props}
                onClick={handleConnect}
                className={cn(
                    "flex items-center gap-2 bg-orange-500 hover:bg-orange-600 text-white font-semibold shadow-lg shadow-orange-500/20",
                    className
                )}
            >
                <Wallet className="h-4 w-4" />
                {buttonText}
            </Button>
        )
    }
)

StarknetConnectModal.displayName = "StarknetConnectModal"

export default StarknetConnectModal
