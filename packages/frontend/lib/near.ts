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
 * Fetch a swap quote from NEAR 1Click chaindefuser API.
 * Uses /v0/quote and reads correlationId per spec §4.
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
            recipient: settlementContract, // Must be destination settlement contract
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

/**
 * Submit source-chain tx hash to NEAR 1Click after the deposit is confirmed.
 * Spec §5 — REQUIRED to reduce NEAR indexer latency from minutes to seconds.
 *
 * Non-fatal: NEAR will detect the deposit automatically via chain indexing
 * if this call fails. Never block the user flow on this.
 */
export async function submitDepositToNear(
    txHash: string,
    depositAddress: string
): Promise<void> {
    try {
        const res = await fetch("https://1click.chaindefuser.com/v0/deposit/submit", {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                Authorization: `Bearer ${NEAR_API_KEY}`,
            },
            body: JSON.stringify({
                txHash,         // 0x-prefixed source chain tx hash
                depositAddress, // From NEAR quote response
            }),
        })

        if (!res.ok) {
            // Non-fatal — log and continue
            const body = await res.text().catch(() => "")
            console.warn("[NEAR] Deposit submit non-fatal failure:", res.status, body)
            return
        }

        const data = await res.json()
        console.log("[NEAR] Deposit submitted:", data.correlationId)
    } catch (err) {
        // Non-fatal — NEAR will still detect the deposit via chain indexing
        console.warn("[NEAR] Deposit submit failed (non-fatal):", err)
    }
}
