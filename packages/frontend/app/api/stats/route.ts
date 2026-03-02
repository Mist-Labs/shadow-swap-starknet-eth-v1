import { createHmac } from "node:crypto"
import { NextResponse } from "next/server"

const RELAYER_URL =
    process.env.NEXT_PUBLIC_RELAYER_URL ||
    process.env.NEXT_PUBLIC_API_BASE_URL ||
    ""

const HMAC_SECRET = process.env.HMAC_SECRET || ""

const EMPTY_STATS = {
    status: "success",
    data: {
        total_intents: 0,
        pending_intents: 0,
        settled_intents: 0,
        failed_intents: 0,
        refunded_intents: 0,
    },
}

/**
 * HMAC-SHA256 using Node.js crypto (server-side only).
 *
 * Per docs: message = timestamp + bodyString
 * For GET requests bodyString = "" (empty string, not "{}").
 * Body must be compact JSON (no spaces) when present.
 */
function sign(timestamp: string, bodyStr: string = ""): string {
    return createHmac("sha256", HMAC_SECRET)
        .update(timestamp + bodyStr, "utf8")
        .digest("hex")
}

/**
 * GET /api/stats
 *
 * Proxies to backend GET /metrics (HMAC-protected, operators only).
 * Uses lowercase header names x-timestamp / x-signature per API docs.
 * Returns zeros on any failure so the UI never crashes.
 */
export async function GET() {
    if (!RELAYER_URL || !HMAC_SECRET) {
        console.warn("[stats proxy] RELAYER_URL or HMAC_SECRET not set — returning zeros")
        return NextResponse.json(EMPTY_STATS, { status: 200 })
    }

    try {
        const timestamp = Math.floor(Date.now() / 1000).toString()
        const signature = sign(timestamp, "") // GET has empty body

        const res = await fetch(`${RELAYER_URL}/metrics`, {
            method: "GET",
            headers: {
                "x-timestamp": timestamp,
                "x-signature": signature,
            },
            signal: AbortSignal.timeout(8_000),
        })

        if (!res.ok) {
            const errBody = await res.text().catch(() => "(unreadable)")
            console.error(`[stats proxy] backend /metrics responded: ${res.status} — ${errBody}`)
            return NextResponse.json(EMPTY_STATS, { status: 200 })
        }

        const data = await res.json()
        return NextResponse.json(data, { status: 200 })
    } catch (err) {
        console.error("[stats proxy] error:", err)
        return NextResponse.json(EMPTY_STATS, { status: 200 })
    }
}
