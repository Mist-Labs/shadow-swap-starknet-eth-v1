import axios from "axios";
import crypto from "crypto";
import { config } from "./config";
import { RelayerEventPayload } from "./types";

/**
 * Generate HMAC signature for relayer authentication
 */
function generateHMACSignature(payload: any): {
  signature: string;
  timestamp: string;
} {
  const requestBody = JSON.stringify(payload);
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const message = timestamp + requestBody;

  const signature = crypto
    .createHmac("sha256", config.hmacSecret)
    .update(message)
    .digest("hex");

  return { signature, timestamp };
}

/**
 * Forward event to relayer backend
 */
export async function forwardToRelayer(payload: RelayerEventPayload): Promise<void> {
  const { signature, timestamp } = generateHMACSignature(payload);

  try {
    console.log(`📤 Forwarding ${payload.event_type} to relayer...`);

    const response = await axios.post(
      `${config.relayerBaseUrl}/indexer/event`,
      payload,
      {
        headers: {
          "Content-Type": "application/json",
          "x-signature": signature,
          "x-timestamp": timestamp,
        },
        timeout: 30000,
      }
    );

    console.log(`✅ Relayer response: ${response.status}`);
  } catch (error: any) {
    console.error("❌ Failed to forward to relayer:", error.response?.data || error.message);
    throw error;
  }
}
