import { NextRequest, NextResponse } from "next/server"

const RELAYER_URL = "https://appropriate-chelsea-mist-labs-1f0a1134.koyeb.app/api/v1"

// All backend intent statuses (v2.0 spec)
const ALL_STATUSES = [
    "pending",
    "batched",
    "near_submitted",
    "tokens_delivered",
    "settled",
    "marked_settled",
    "failed",
    "refunded",
]

/**
 * Fetch intents for a single status value from the backend.
 * Returns empty array on 404 or any non-ok response (never throws).
 */
async function fetchByStatus(
    status: string,
    limit: number,
    userAddress?: string
): Promise<Record<string, unknown>[]> {
    if (!RELAYER_URL) return []

    const params = new URLSearchParams({ status, limit: String(limit) })
    if (userAddress) params.set("user_address", userAddress)

    try {
        const res = await fetch(`${RELAYER_URL}/bridge/intents?${params}`, {
            headers: { "Content-Type": "application/json" },
            signal: AbortSignal.timeout(10_000),
        })

        if (res.status === 404) return []
        if (!res.ok) {
            const text = await res.text().catch(() => "")
            console.error(`[intents proxy] ${status} → ${res.status}:`, text)
            return []
        }

        const body = (await res.json()) as { data?: Record<string, unknown>[] }
        return Array.isArray(body?.data) ? body.data : []
    } catch (err) {
        if (err instanceof Error && err.name !== "TimeoutError") {
            console.error(`[intents proxy] fetch error for status=${status}:`, err)
        }
        return []
    }
}

/**
 * GET /api/intents
 *
 * Query params:
 *   status        – single status OR omit for all statuses
 *   limit         – max results (default 50, applied per-status when fetching all)
 *   user_address  – optional address filter
 *   view_key      – optional view key (forwarded to backend when supported)
 *
 * Proxies to the backend relayer so the client never needs the backend URL.
 * The backend REQUIRES a ?status param — when omitted here we fan-out across
 * all statuses and merge the results.
 */
export async function GET(req: NextRequest) {
    try {
        if (!RELAYER_URL) {
            return NextResponse.json(
                { status: "success", count: 0, data: [] },
                { status: 200 }
            )
        }

        const { searchParams } = req.nextUrl
        const requestedStatus = searchParams.get("status") || ""
        const limit = Math.min(
            parseInt(searchParams.get("limit") || "50", 10),
            200
        )
        const userAddress = searchParams.get("user_address") || undefined

        let data: Record<string, unknown>[]

        if (requestedStatus) {
            // Single status requested
            data = await fetchByStatus(requestedStatus, limit, userAddress)
        } else {
            // No status filter — fan-out across all statuses and merge
            const results = await Promise.all(
                ALL_STATUSES.map((s) => fetchByStatus(s, limit, userAddress))
            )
            data = results.flat()

            // Sort merged results by created_at descending (newest first)
            data.sort((a, b) => {
                const ta = (a.created_at as number) || 0
                const tb = (b.created_at as number) || 0
                return tb - ta
            })

            // Apply global limit after merge
            data = data.slice(0, limit)
        }

        return NextResponse.json(
            { status: "success", count: data.length, data },
            { status: 200 }
        )
    } catch (err) {
        console.error("[intents proxy] unexpected error:", err)
        return NextResponse.json(
            { status: "success", count: 0, data: [] },
            { status: 200 }
        )
    }
}
