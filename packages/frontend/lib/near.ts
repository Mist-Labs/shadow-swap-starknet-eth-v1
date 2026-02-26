export interface NearQuoteRequest {
    originAsset: string
    destinationAsset: string
    amount: string
    settlementContract: string
    refundTo: string
}

export interface NearQuoteResponse {
    near_intents_id: string
    deposit_address: string
    min_amount_out: string
}

const NEAR_API_KEY = process.env.NEXT_PUBLIC_NEAR_API_KEY || ""

/**
 * Fetch a swap quote from NEAR 1Click chaindefuser API
 * Uses v0/quote instead of v0/swap natively
 */
export async function createNearSwapQuote({
    originAsset,
    destinationAsset,
    amount,
    settlementContract,
    refundTo,
}: NearQuoteRequest): Promise<NearQuoteResponse> {
    const deadline = new Date(Date.now() + 30 * 60 * 1000).toISOString()

    const res = await fetch("https://1click.chaindefuser.com/v0/quote", {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${NEAR_API_KEY}`,
        },
        body: JSON.stringify({
            dry: false,
            swapType: "EXACT_INPUT",
            slippageTolerance: 100, // 1%
            originAsset,
            destinationAsset,
            amount,
            recipient: settlementContract, // Must be destination settlement
            refundTo,
            depositType: "ORIGIN_CHAIN",
            refundType: "ORIGIN_CHAIN",
            recipientType: "DESTINATION_CHAIN",
            referral: "shadowswap",
            deadline,
        }),
    })

    if (!res.ok) {
        let errorMsg = "Failed to fetch NEAR quote"
        try {
            const error = await res.json()
            errorMsg = error.message || error.error || errorMsg
        } catch { } // ignore
        throw new Error(errorMsg)
    }

    const data = await res.json()

    return {
        near_intents_id: data.correlationId, // MUST use correlationId, NOT data.id
        deposit_address: data.quote.depositAddress,
        min_amount_out: data.quote.minAmountOut,
    }
}
