// Goldsky webhook payload structure
export interface GoldskyWebhookPayload {
  entity: string;
  data: {
    new: Record<string, any>;
    old?: Record<string, any>;
  };
  metadata?: {
    chain_id?: string;
  };
}

// ShadowSwap event types (matches Goldsky entity names from Solidity events)
export type EventType =
  | "commitment_added"
  | "intent_settled"
  | "intent_marked_settled"
  | "merkle_root_updated"
  | "batch_processed";

// Chain types
export enum Chain {
  Ethereum = "ethereum",
  Sepolia = "sepolia",
}

// Relayer event payload (what we send to backend)
export interface RelayerEventPayload {
  event_type: EventType;
  chain: string;
  transaction_hash: string;
  block_number: number;
  log_index: number;
  event_data: Record<string, any>;
}
