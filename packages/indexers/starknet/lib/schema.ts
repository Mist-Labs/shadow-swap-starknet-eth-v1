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

export const intents_settled = pgTable("intents_settled", {
  id: serial("id").primaryKey(),
  eventId: text("event_id").notNull().unique(),
  blockNumber: bigint("block_number", { mode: "number" }).notNull(),
  timestamp: timestamp("timestamp").notNull(),
  transactionHash: text("transaction_hash").notNull(),
  intentId: text("intent_id").notNull(),
  nullifierHash: text("nullifier_hash").notNull(),
  token: text("token").notNull(),
  amountLow: text("amount_low").notNull(),
  amountHigh: text("amount_high").notNull(),
  eventTimestamp: bigint("event_timestamp", { mode: "number" }).notNull(),
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
