"use client"

import "@/lib/web3/config" // registers the Reown AppKit modal (side-effect)
import { config, queryClient } from "@/lib/web3/config"
import { WagmiProvider } from "wagmi"
import { QueryClientProvider } from "@tanstack/react-query"
import { StarknetProvider } from "@/components/starknet-provider"
import type React from "react"

export default function Providers({ children }: { children: React.ReactNode }) {
    return (
        <WagmiProvider config={config}>
            <QueryClientProvider client={queryClient}>
                <StarknetProvider>
                    {children}
                </StarknetProvider>
            </QueryClientProvider>
        </WagmiProvider>
    )
}
