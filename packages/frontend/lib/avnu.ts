import { getQuotes, quoteToCalls, type Quote } from "@avnu/avnu-sdk"
import { type Call, cairo } from "starknet"

/**
 * AVNU DEX router on StarkNet Mainnet.
 * Docs: https://doc.avnu.fi/
 */
export const AVNU_SLIPPAGE = 0.005 // 0.5% safety margin

export interface AvnuQuoteResult {
    /** STRK amount the user will receive (as decimal string in atomic units) */
    strkAmount: string
    /** The full quote object from AVNU SDK */
    quote: Quote
}

/**
 * Fetch the best AVNU swap quote using the official SDK.
 */
export async function getAvnuQuote(
    sellTokenAddress: string,
    buyTokenAddress: string,
    sellAmount: string,
    takerAddress: string
): Promise<AvnuQuoteResult> {
    const quotes = await getQuotes({
        sellTokenAddress,
        buyTokenAddress,
        sellAmount: BigInt(sellAmount),
        takerAddress,
    })

    if (!quotes || quotes.length === 0) {
        throw new Error("No swap route found on AVNU.")
    }

    const best = quotes[0]
    return {
        strkAmount: best.buyAmount.toString(),
        quote: best,
    }
}

/**
 * Build the execution call for an AVNU swap using the SDK.
 */
export async function getAvnuSwapCall(
    quote: Quote,
    takerAddress: string,
    slippage: number = AVNU_SLIPPAGE
): Promise<Call> {
    const { calls } = await quoteToCalls({
        quoteId: quote.quoteId,
        slippage,
        takerAddress,
    })

    if (!calls || calls.length === 0) {
        throw new Error("Failed to generate swap calls from AVNU SDK.")
    }

    // Return the first call (multi_route_swap)
    // Ensure all calldata entries are strings to prevent 'invalid_union' errors
    return {
        contractAddress: calls[0].contractAddress,
        entrypoint: calls[0].entrypoint,
        calldata: (calls[0].calldata as (string | bigint | number)[]).map((v) => v.toString()),
    }
}

/**
 * Build the 3-call StarkNet multicall.
 */
export function buildMulticall(
    sellTokenAddress: string,
    strkAddress: string,
    sellAmount: string,
    strkAmount: string,
    swapCall: Call,
    depositAddress: string
): Call[] {
    return [
        // 1. Approve sellToken to AVNU router
        {
            contractAddress: sellTokenAddress,
            entrypoint: "approve",
            calldata: [
                swapCall.contractAddress,
                cairo.uint256(sellAmount).low,
                cairo.uint256(sellAmount).high,
            ].map((v) => v.toString()),
        },

        // 2. Execute AVNU swap call
        swapCall,

        // 3. Transfer STRK to NEAR deposit address
        {
            contractAddress: strkAddress,
            entrypoint: "transfer",
            calldata: [
                depositAddress,
                cairo.uint256(strkAmount).low,
                cairo.uint256(strkAmount).high,
            ].map((v) => v.toString()),
        },
    ]
}
