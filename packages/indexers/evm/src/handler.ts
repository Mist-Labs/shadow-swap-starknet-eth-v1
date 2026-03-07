import { GoldskyWebhookPayload, RelayerEventPayload, EventType } from "./types";
import { deriveChainId, normalizeHex } from "./utils";

/**
 * Parse event data based on entity type
 */
function parseEventData(eventData: any, entity: string): Record<string, any> {
  switch (entity) {
    case "commitment_added":
      return {
        commitment: normalizeHex(eventData.commitment),
        intentId: eventData.intent_id
          ? normalizeHex(eventData.intent_id)
          : undefined,
      };

    case "intent_settled":
      return {
        intentId: eventData.intent_id
          ? normalizeHex(eventData.intent_id)
          : undefined,
        nullifierHash: normalizeHex(
          eventData.nullifier_hash || eventData.nullifierHash,
        ),
        token: normalizeHex(eventData.token),
        recipient: normalizeHex(eventData.recipient),
        amount: eventData.amount,
      };

    case "intent_marked_settled":
      return {
        commitment: normalizeHex(eventData.commitment),
        nullifierHash: normalizeHex(
          eventData.nullifier_hash || eventData.nullifierHash,
        ),
      };

    case "merkle_root_updated":
      return {
        newRoot: normalizeHex(eventData.new_root || eventData.root),
      };

    case "batch_processed":
      return {
        batchId: eventData.batch_id,
        commitmentsCount: eventData.commitments_count,
        reason: eventData.reason,
      };

    default:
      return {};
  }
}

/**
 * Transform Goldsky webhook payload to relayer format
 */
export async function transformGoldskyPayload(
  payload: GoldskyWebhookPayload,
): Promise<RelayerEventPayload> {
  const { entity, data } = payload;
  const eventData = data.new;

  const chainId = deriveChainId(payload);

  return {
    event_type: entity as EventType,
    chain:
      chainId === "1" || chainId === "ethereum"
        ? "ethereum"
        : chainId === "11155111" || chainId === "sepolia"
          ? "sepolia"
          : (() => {
              throw new Error(`Unsupported chain: ${chainId}`);
            })(),
    transaction_hash: eventData.transaction_hash,
    block_number: parseInt(eventData.block_number),
    log_index: parseInt(
      eventData.log_index || eventData.id?.split("-")[1] || "0",
    ),
    event_data: parseEventData(eventData, entity),
  };
}
