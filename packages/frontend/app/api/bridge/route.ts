import { createHmac } from "node:crypto"
import { NextResponse } from "next/server"

const RELAYER_URL = "https://appropriate-chelsea-mist-labs-1f0a1134.koyeb.app/api/v1"

const HMAC_SECRET = process.env.HMAC_SECRET || ""

/**
 * Sign a request body using HMAC-SHA256.
 * Per docs: message = timestamp + bodyString (compact JSON, no spaces).
 * Server-side only — HMAC_SECRET never exposed to browser.
 */
function sign(timestamp: string, bodyStr: string): string {
    return createHmac("sha256", HMAC_SECRET)
        .update(timestamp + bodyStr, "utf8")
        .digest("hex")
}

export async function POST(req: Request) {
    if (!RELAYER_URL || !HMAC_SECRET) {
        return NextResponse.json(
            { error: "Backend not configured" },
            { status: 503 }
        )
    }

    try {
        const payload = await req.json()
        const timestamp = Math.floor(Date.now() / 1000).toString()
        // Must use the SAME bodyString for signing and for the request body
        const bodyString = JSON.stringify(payload) // compact JSON, no spaces
        
        console.log("[bridge proxy] Sending payload to backend:", bodyString)

        const signature = sign(timestamp, bodyString)

        const response = await fetch(`${RELAYER_URL}/bridge/initiate`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "x-timestamp": timestamp,
                "x-signature": signature,
            },
            body: bodyString,
        })

        const text = await response.text()
        if (!response.ok) {
            console.error(`[bridge proxy] Backend returned ${response.status}:`, text)
        }
        
        let data
        try {
            data = JSON.parse(text)
        } catch {
            data = { error: "Non-JSON response from backend", details: text }
        }

        return NextResponse.json(data, { status: response.status })
    } catch (error: unknown) {
        console.error("[bridge proxy] error:", error)
        return NextResponse.json(
            { error: "Internal Server Error", message: error instanceof Error ? error.message : "Unknown error" },
            { status: 500 }
        )
    }
}
