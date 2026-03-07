import { NextRequest, NextResponse } from "next/server"

const RELAYER_URL =
    process.env.NEXT_PUBLIC_RELAYER_URL ||
    process.env.NEXT_PUBLIC_API_BASE_URL ||
    ""

/**
 * GET /api/intent/[id]
 *
 * Proxies to backend GET /bridge/intent/{id} (no auth required).
 * Returns 404 JSON if the intent is not found.
 */
export async function GET(
    _req: NextRequest,
    { params }: { params: Promise<{ id: string }> }
) {
    const { id } = await params

    if (!RELAYER_URL) {
        return NextResponse.json(
            { success: false, message: "Backend not configured" },
            { status: 503 }
        )
    }

    try {
        const res = await fetch(
            `${RELAYER_URL}/bridge/intent/${encodeURIComponent(id)}`,
            { signal: AbortSignal.timeout(8_000) }
        )

        const data = await res.json()
        return NextResponse.json(data, { status: res.status })
    } catch (err) {
        console.error("[intent proxy] error:", err)
        return NextResponse.json(
            { success: false, message: "Failed to fetch intent" },
            { status: 500 }
        )
    }
}
