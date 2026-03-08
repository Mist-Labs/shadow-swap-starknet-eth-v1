"use client"

import React from "react"
import { mainnet } from "@starknet-react/chains"
import {
    StarknetConfig,
    jsonRpcProvider,
    voyager,
} from "@starknet-react/core"
import { InjectedConnector } from "starknetkit/injected"
import { WebWalletConnector } from "starknetkit/webwallet"
import type { Connector } from "@starknet-react/core"

const ARGENT_WEBWALLET_URL = "https://web.argent.xyz"

const connectors = [
    new InjectedConnector({ options: { id: "argentX" } }),
    new InjectedConnector({ options: { id: "braavos" } }),
    new WebWalletConnector({ url: ARGENT_WEBWALLET_URL }),
]

// Use the project's Alchemy Starknet RPC (v0_10 — latest spec, compatible with starknet.js v9)
const provider = jsonRpcProvider({
    rpc: () => ({
        nodeUrl: process.env.NEXT_PUBLIC_STARKNET_RPC_URL ||
            "https://starknet-mainnet.g.alchemy.com/starknet/version/rpc/v0_10/vln-XygYhSjsuwzmVcyWH",
    }),
})

export function StarknetProvider({ children }: { children: React.ReactNode }) {
    return (
        <StarknetConfig
            chains={[mainnet]}
            provider={provider}
            connectors={connectors as Connector[]}
            explorer={voyager}
        >
            {children}
        </StarknetConfig>
    )
}
