import { CallData, type Call } from "starknet"

/**
 * AVNU DEX router on StarkNet Mainnet.
 * Docs: https://doc.avnu.fi/
 */
const AVNU_ROUTER = "0x04270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f"
const AVNU_API = "https://starknet.api.avnu.fi"

export interface AvnuQuoteResult {
    /** STRK amount the user will receive (as decimal string in atomic units) */
    strkAmount: string
    /** Raw route calldata for the AVNU swap call */
    routeCalldata: string[]
}

/**
 * Fetch the best AVNU swap quote: sellToken → STRK.
 * Used before bridging USDT or USDC from StarkNet via NEAR.
 *
 * @param sellTokenAddress - StarkNet address of the token to sell (USDT or USDC)
 * @param strkAddress      - STRK contract address
 * @param sellAmount       - Amount in atomic units (e.g. "1000000" for 1 USDT)
 * @param userAddress      - User's StarkNet address (STRK returned here)
 */
export async function getAvnuQuote(
    sellTokenAddress: string,
    strkAddress: string,
    sellAmount: string,
    userAddress: string,
    slippageTolerance: number = 100  // 100 = 1%, per spec §multicall notes
): Promise<AvnuQuoteResult> {
    const sellAmountHex = "0x" + BigInt(sellAmount).toString(16)
    
    const params = new URLSearchParams({
        sellTokenAddress,
        buyTokenAddress: strkAddress,
        sellAmount: sellAmountHex,
        takerAddress: userAddress,
        slippageTolerance: slippageTolerance.toString(),
    })

    const res = await fetch(`${AVNU_API}/swap/v2/quotes?${params.toString()}`, {
        method: "GET",
        headers: { "Content-Type": "application/json" }
    })

    if (!res.ok) {
        const err = await res.text().catch(() => "")
        throw new Error(`AVNU quote failed (${res.status}): ${err}`)
    }

    const data = await res.json()

    if (!Array.isArray(data) || data.length === 0) {
        throw new Error("No swap route found on AVNU. Insufficient liquidity or amount too small.")
    }

    const best = data[0]
    return {
        // Convert buyAmount from hex (e.g. "0xf4240") back to base-10 string
        strkAmount: BigInt(best.buyAmount).toString(10),
        routeCalldata: best.routes[0].calldata as string[],
    }
}

/**
 * Build the 3-call StarkNet multicall:
 *   1. approve(sellToken → AVNU router)
 *   2. multi_route_swap (AVNU: sellToken → STRK, STRK returned to user)
 *   3. transfer(STRK → NEAR deposit address)
 *
 * All 3 execute atomically — if any reverts, no funds move.
 *
 * @param sellTokenAddress - USDT or USDC contract address on StarkNet
 * @param strkAddress      - STRK contract address
 * @param sellAmount       - Amount to sell in atomic units
 * @param strkAmount       - STRK amount to forward (from AVNU quote)
 * @param routeCalldata    - Swap calldata from AVNU quote
 * @param depositAddress   - NEAR deposit address (destination for STRK)
 */
export function buildMulticall(
    sellTokenAddress: string,
    strkAddress: string,
    sellAmount: string,
    strkAmount: string,
    routeCalldata: string[],
    depositAddress: string
): Call[] {
    return [
        // 1. Approve USDT/USDC to AVNU router
        {
            contractAddress: sellTokenAddress,
            entrypoint: "approve",
            calldata: CallData.compile({
                spender: AVNU_ROUTER,
                amount: { low: sellAmount, high: 0 },
            }),
        },

        // 2. Execute AVNU swap (sellToken → STRK, STRK lands in user wallet)
        {
            contractAddress: AVNU_ROUTER,
            entrypoint: "multi_route_swap",
            calldata: routeCalldata,
        },

        // 3. Transfer STRK to NEAR deposit address (initiates bridge)
        {
            contractAddress: strkAddress,
            entrypoint: "transfer",
            calldata: CallData.compile({
                recipient: depositAddress,
                amount: { low: strkAmount, high: 0 },
            }),
        },
    ]
}
