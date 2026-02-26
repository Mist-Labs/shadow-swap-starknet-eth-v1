import { NextResponse } from "next/server"

const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_BASE_URL ||
  "https://international-linnie-mist-labs-2c5cd590.koyeb.app/api/v1"

const HMAC_SECRET = process.env.HMAC_SECRET

async function generateHMACSignature(
  payload: Record<string, unknown>,
  timestamp: string
): Promise<string> {
  if (!HMAC_SECRET) {
    throw new Error("HMAC secret not configured. Please set HMAC_SECRET environment variable.")
  }

  const requestBody = JSON.stringify(payload)
  const message = timestamp + requestBody

  const encoder = new TextEncoder()
  const keyData = encoder.encode(HMAC_SECRET)
  const messageData = encoder.encode(message)

  const key = await crypto.subtle.importKey(
    "raw",
    keyData,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  )

  const signature = await crypto.subtle.sign("HMAC", key, messageData)
  const hashArray = Array.from(new Uint8Array(signature))
  return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("")
}

export async function POST(req: Request) {
  try {
    const payload = await req.json()
    const timestamp = Math.floor(Date.now() / 1000).toString()

    const signature = await generateHMACSignature(payload, timestamp)

    const response = await fetch(`${API_BASE_URL}/bridge/initiate`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Signature": signature,
        "X-Timestamp": timestamp,
      },
      body: JSON.stringify(payload),
    })

    const data = await response.json()
    return NextResponse.json(data, { status: response.status })
  } catch (error: unknown) {
    console.error("Bridge API Proxy Error:", error)
    return NextResponse.json(
      { error: "Internal Server Error", message: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    )
  }
}
