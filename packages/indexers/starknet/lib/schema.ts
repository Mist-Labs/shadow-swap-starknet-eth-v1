//  --- ShadowSwap StarkNet Event Schemas ----

import {
  serial,
  pgTable,
  text,
  bigint,
  timestamp,
  boolean,
} from "drizzle-orm/pg-core";

export const commitments = pgTable("commitments", {
  id: serial("id").primaryKey(),
  eventId: text("event_id").notNull().unique(),
  blockNumber: bigint("block_number", { mode: "number" }).notNull(),
  timestamp: timestamp("timestamp").notNull(),
  transactionHash: text("transaction_hash").notNull(),
  commitment: text("commitment").notNull(),
});

// Covers both IntentSettled (direct STRK delivery) and IntentSettledWithSwap
// (STRK → dest_token via AVNU).
//
// Direct settlement:  token/amountLow/amountHigh = what recipient received.
//                     isSwap = false. delivered* columns = null.
//
// Swap settlement:    token/amountLow/amountHigh = dest_token/dest_amount
//                     (what recipient actually received after swap).
//                     isSwap = true.
//                     deliveredToken/deliveredAmountLow/deliveredAmountHigh =
//                     the STRK that arrived from NEAR before the AVNU swap.
export const intents_settled = pgTable("intents_settled", {
  id: serial("id").primaryKey(),
  eventId: text("event_id").notNull().unique(),
  blockNumber: bigint("block_number", { mode: "number" }).notNull(),
  timestamp: timestamp("timestamp").notNull(),
  transactionHash: text("transaction_hash").notNull(),
  intentId: text("intent_id").notNull(),
  nullifierHash: text("nullifier_hash").notNull(),
  // Canonical received token + amount (dest_token for swaps, STRK for direct)
  token: text("token").notNull(),
  amountLow: text("amount_low").notNull(),
  amountHigh: text("amount_high").notNull(),
  eventTimestamp: bigint("event_timestamp", { mode: "number" }).notNull(),
  // Swap-only fields — null for direct STRK settlements
  isSwap: boolean("is_swap").notNull().default(false),
  deliveredToken: text("delivered_token"),
  deliveredAmountLow: text("delivered_amount_low"),
  deliveredAmountHigh: text("delivered_amount_high"),
});

export const intents_marked_settled = pgTable("intents_marked_settled", {
  id: serial("id").primaryKey(),
  eventId: text("event_id").notNull().unique(),
  blockNumber: bigint("block_number", { mode: "number" }).notNull(),
  timestamp: timestamp("timestamp").notNull(),
  transactionHash: text("transaction_hash").notNull(),
  nullifierHash: text("nullifier_hash").notNull(),
  commitment: text("commitment").notNull(),
  eventTimestamp: bigint("event_timestamp", { mode: "number" }).notNull(),
});

export const merkle_roots = pgTable("merkle_roots", {
  id: serial("id").primaryKey(),
  eventId: text("event_id").notNull().unique(),
  blockNumber: bigint("block_number", { mode: "number" }).notNull(),
  timestamp: timestamp("timestamp").notNull(),
  transactionHash: text("transaction_hash").notNull(),
  newRoot: text("new_root").notNull(),
});

export const batches_processed = pgTable("batches_processed", {
  id: serial("id").primaryKey(),
  eventId: text("event_id").notNull().unique(),
  blockNumber: bigint("block_number", { mode: "number" }).notNull(),
  timestamp: timestamp("timestamp").notNull(),
  transactionHash: text("transaction_hash").notNull(),
  batchId: bigint("batch_id", { mode: "number" }).notNull(),
  commitmentsCount: bigint("commitments_count", { mode: "number" }).notNull(),
  reason: text("reason").notNull(),
});