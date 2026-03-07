import { NextResponse } from "next/server"

const RELAYER_URL =
    process.env.NEXT_PUBLIC_RELAYER_URL ||
    process.env.NEXT_PUBLIC_API_BASE_URL ||
    ""

/**
 * GET /api/health
 * Proxies to the backend relayer /health endpoint.
 * Always returns 200 — error details go in the body so the client
 * can show a degraded state without the UI throwing.
 */
export async function GET() {
    if (!RELAYER_URL) {
        return NextResponse.json(
            { status: "unavailable", timestamp: new Date().toISOString(), components: {} },
            { status: 200 }
        )
    }

    try {
        const res = await fetch(`${RELAYER_URL}/health`, {
            signal: AbortSignal.timeout(8_000),
        })

        const data = await res.json()
        return NextResponse.json(data, { status: 200 })
    } catch (err) {
        console.error("[health proxy] error:", err)
        return NextResponse.json(
            { status: "unavailable", timestamp: new Date().toISOString(), components: {} },
            { status: 200 }
        )
    }
}
