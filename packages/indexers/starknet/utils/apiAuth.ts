import crypto from "crypto";
import axios from "axios";

export function validateApiVariables(): { isValid: boolean; missingVars: string[] } {
  const requiredVars = ["RELAYER_BASE_URL", "HMAC_SECRET"];
  const missingVars = requiredVars.filter((v) => !process.env[v]);

  return {
    isValid: missingVars.length === 0,
    missingVars,
  };
}

function generateHMACSignature(payload: any): { signature: string; timestamp: string } {
  const secret = process.env.HMAC_SECRET!;
  // Relayer validates with Utc::now().timestamp() which is seconds, not ms
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const message = timestamp + JSON.stringify(payload);
  const signature = crypto.createHmac("sha256", secret).update(message).digest("hex");

  return { signature, timestamp };
}

export async function forwardToRelayer(
  eventType: string,
  eventData: any,
  transactionHash: string,
  logger: any,
  blockNumber: number = 0,
  logIndex: number = 0,
): Promise<boolean> {
  try {
    const payload = {
      event_type: eventType,
      chain: "starknet",
      transaction_hash: transactionHash,
      block_number: blockNumber,
      log_index: logIndex,
      event_data: eventData,
    };

    const { signature, timestamp } = generateHMACSignature(payload);
    const relayerUrl = process.env.RELAYER_BASE_URL!;

    const response = await axios.post(`${relayerUrl}/indexer/event`, payload, {
      headers: {
        "Content-Type": "application/json",
        "x-signature": signature,
        "x-timestamp": timestamp,
      },
      timeout: 10000,
    });

    if (response.status === 200) {
      logger.info(`✅ Forwarded ${eventType} to relayer:`, {
        status: response.data.status,
      });
      return true;
    } else {
      logger.warn(`⚠️  Relayer responded with status ${response.status}`);
      return false;
    }
  } catch (error: any) {
    logger.error(`❌ Failed to forward ${eventType} to relayer:`, {
      error: error.message,
      response: error.response?.data,
    });
    return false;
  }
}
