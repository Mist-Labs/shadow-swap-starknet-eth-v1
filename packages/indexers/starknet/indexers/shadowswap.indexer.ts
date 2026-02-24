import { defineIndexer } from "apibara/indexer";
import { useLogger } from "apibara/plugins";
import {
  commitments,
  intents_settled,
  intents_marked_settled,
  merkle_roots,
  batches_processed,
} from "../lib/schema";
import { useDrizzleStorage, drizzleStorage } from "@apibara/plugin-drizzle";
import { drizzle } from "@apibara/plugin-drizzle";
import { StarknetStream } from "@apibara/starknet";
import type { ApibaraRuntimeConfig } from "apibara/types";
import { hash, uint256 } from "starknet";
import { forwardToRelayer, validateApiVariables } from "../utils/apiAuth.js";
import { eq, desc } from "drizzle-orm";

// All events emitted by ShadowSettlement (ShadowSwap Cairo contract).
// Key formula: sn_keccak(PascalCaseName) — confirmed on-chain.
// Indexed = stored in DB + forwarded to relayer.
// Ignored = emitted by contract but not relevant to relayer lifecycle.
const ALL_EVENTS = {
  // --- Indexed ---
  CommitmentAdded:      { indexed: true },
  BatchProcessed:       { indexed: true },
  MerkleRootUpdated:    { indexed: true },
  IntentSettled:        { indexed: true },
  IntentMarkedSettled:  { indexed: true },
  // --- Ignored ---
  RemoteRootSynced:         { indexed: false },
  RemoteRootVerified:       { indexed: false },
  RelayerStatusChanged:     { indexed: false },
  RootVerifierStatusChanged:{ indexed: false },
  TokenWhitelistUpdated:    { indexed: false },
  BatchConfigUpdated:       { indexed: false },
  Paused:                   { indexed: false },
  Unpaused:                 { indexed: false },
} as const;

// Build selector → event name map at startup (sn_keccak of each name).
// sn_keccak yields a 250-bit value — may be fewer than 64 hex chars.
// Apibara always zero-pads to 64 hex chars (32 bytes), so we normalise
// every selector to that canonical form before storing and before lookup.
const pad64 = (sel: string) => "0x" + sel.slice(2).padStart(64, "0");

const SELECTOR_TO_NAME: Record<string, string> = {};
for (const name of Object.keys(ALL_EVENTS)) {
  const sel = pad64(hash.getSelectorFromName(name));
  SELECTOR_TO_NAME[sel] = name;
}

// Typed keys for the five indexed events.
// Must use pad64() to match apibara's zero-padded 64-char selector format.
const EVENT_KEYS = {
  CommitmentAdded:     pad64(hash.getSelectorFromName("CommitmentAdded")),
  IntentSettled:       pad64(hash.getSelectorFromName("IntentSettled")),
  IntentMarkedSettled: pad64(hash.getSelectorFromName("IntentMarkedSettled")),
  MerkleRootUpdated:   pad64(hash.getSelectorFromName("MerkleRootUpdated")),
  BatchProcessed:      pad64(hash.getSelectorFromName("BatchProcessed")),
};

interface CommitmentAddedEvent {
  commitment: string;
}

interface IntentSettledEvent {
  intent_id: string;
  nullifier_hash: string;
  token: string;
  amount: bigint;
  timestamp: bigint;
}

interface IntentMarkedSettledEvent {
  nullifier_hash: string;
  commitment: string;
  timestamp: bigint;
}

interface MerkleRootUpdatedEvent {
  new_root: string;
}

interface BatchProcessedEvent {
  batch_id: bigint;
  commitments_count: bigint;
  reason: string;
}

async function getLastIndexedBlock(
  db: any,
  logger: any,
  fallbackBlock: number
): Promise<bigint> {
  try {
    const result = await db
      .select({ blockNumber: commitments.blockNumber })
      .from(commitments)
      .orderBy(desc(commitments.blockNumber))
      .limit(1);

    if (result.length > 0) {
      logger.info(`[RESUME] Last indexed block: ${result[0].blockNumber}`);
      return BigInt(result[0].blockNumber + 1);
    }
  } catch (error: any) {
    logger.warn(`[RESUME] Could not retrieve last indexed block: ${error?.message}`);
  }

  logger.info(`[RESUME] Starting from configured block: ${fallbackBlock}`);
  return BigInt(fallbackBlock);
}

function isEventType(event: any, eventKey: string): boolean {
  return event.keys?.some((key: string) => pad64(key) === eventKey) || false;
}

function decodeCommitmentAdded(event: any, logger: any): CommitmentAddedEvent | null {
  try {
    const { keys } = event;
    if (!keys || keys.length < 2) {
      throw new Error(`Expected >=2 keys, got ${keys?.length ?? 0}`);
    }
    // keys: [selector, commitment]
    const decoded = { commitment: keys[1] };
    logger.info(`[DECODE] CommitmentAdded commitment=${decoded.commitment}`);
    return decoded;
  } catch (error: any) {
    logger.error(`[DECODE ERROR] CommitmentAdded: ${error?.message}`, {
      keys: event.keys,
      data: event.data,
    });
    return null;
  }
}

function decodeIntentSettled(event: any, logger: any): IntentSettledEvent | null {
  try {
    const { keys, data } = event;
    if (!keys || keys.length < 3 || !data || data.length < 4) {
      throw new Error(`Expected keys>=3 data>=4, got keys=${keys?.length} data=${data?.length}`);
    }
    // keys: [selector, intent_id, nullifier_hash]
    // data: [token, amount_low, amount_high, timestamp]
    const amount = uint256.uint256ToBN({ low: data[1], high: data[2] });
    const decoded = {
      intent_id: keys[1],
      nullifier_hash: keys[2],
      token: data[0],
      amount,
      timestamp: BigInt(data[3]),
    };
    logger.info(`[DECODE] IntentSettled intent_id=${decoded.intent_id} nullifier=${decoded.nullifier_hash} token=${decoded.token} amount=${decoded.amount}`);
    return decoded;
  } catch (error: any) {
    logger.error(`[DECODE ERROR] IntentSettled: ${error?.message}`, {
      keys: event.keys,
      data: event.data,
    });
    return null;
  }
}

function decodeIntentMarkedSettled(event: any, logger: any): IntentMarkedSettledEvent | null {
  try {
    const { keys, data } = event;
    if (!keys || keys.length < 3 || !data || data.length < 1) {
      throw new Error(`Expected keys>=3 data>=1, got keys=${keys?.length} data=${data?.length}`);
    }
    // keys: [selector, nullifier_hash, commitment]
    // data: [timestamp]
    const decoded = {
      nullifier_hash: keys[1],
      commitment: keys[2],
      timestamp: BigInt(data[0]),
    };
    logger.info(`[DECODE] IntentMarkedSettled nullifier=${decoded.nullifier_hash} commitment=${decoded.commitment}`);
    return decoded;
  } catch (error: any) {
    logger.error(`[DECODE ERROR] IntentMarkedSettled: ${error?.message}`, {
      keys: event.keys,
      data: event.data,
    });
    return null;
  }
}

function decodeMerkleRootUpdated(event: any, logger: any): MerkleRootUpdatedEvent | null {
  try {
    const { keys } = event;
    if (!keys || keys.length < 2) {
      throw new Error(`Expected >=2 keys, got ${keys?.length ?? 0}`);
    }
    // keys: [selector, new_root]
    const decoded = { new_root: keys[1] };
    logger.info(`[DECODE] MerkleRootUpdated new_root=${decoded.new_root}`);
    return decoded;
  } catch (error: any) {
    logger.error(`[DECODE ERROR] MerkleRootUpdated: ${error?.message}`, {
      keys: event.keys,
      data: event.data,
    });
    return null;
  }
}

function decodeBatchProcessed(event: any, logger: any): BatchProcessedEvent | null {
  try {
    const { keys, data } = event;
    if (!keys || keys.length < 2 || !data || data.length < 2) {
      throw new Error(`Expected keys>=2 data>=2, got keys=${keys?.length} data=${data?.length}`);
    }
    // keys: [selector, batch_id]
    // data: [commitments_count, reason_enum]
    const reasonEnum = Number(data[1]);
    const reason = reasonEnum === 0 ? "BatchFull" : "TimeoutReached";
    const decoded = {
      batch_id: BigInt(keys[1]),
      commitments_count: BigInt(data[0]),
      reason,
    };
    logger.info(`[DECODE] BatchProcessed batch_id=${decoded.batch_id} count=${decoded.commitments_count} reason=${decoded.reason}`);
    return decoded;
  } catch (error: any) {
    logger.error(`[DECODE ERROR] BatchProcessed: ${error?.message}`, {
      keys: event.keys,
      data: event.data,
    });
    return null;
  }
}

interface ShadowSwapConfig {
  startingBlock: number;
  streamUrl: string;
  contractAddress: string;
}

export default function (runtimeConfig: ApibaraRuntimeConfig) {
  const { startingBlock, streamUrl, contractAddress } =
    (runtimeConfig["shadowswap"] as ShadowSwapConfig);

  const db = drizzle({
    schema: {
      commitments,
      intents_settled,
      intents_marked_settled,
      merkle_roots,
      batches_processed,
    },
  });

  let hasCheckedResume = false;

  return defineIndexer(StarknetStream)({
    streamUrl,
    finality: "accepted",
    startingBlock: BigInt(startingBlock),
    filter: {
      header: "on_data",
      events: [
        {
          address: contractAddress as `0x${string}`,
        },
      ],
    },
    plugins: [drizzleStorage({ db })],

    async transform({ block }) {
      const logger = useLogger();

      if (!hasCheckedResume) {
        try {
          const { db: storageDb } = useDrizzleStorage();
          const lastBlock = await getLastIndexedBlock(storageDb, logger, startingBlock);

          if (lastBlock > BigInt(startingBlock)) {
            logger.info(`[RESUME] Resuming from block ${lastBlock}`);
            if (BigInt(block.header.blockNumber) < lastBlock) {
              logger.info(`[RESUME] Skipping block ${block.header.blockNumber} (before resume point ${lastBlock})`);
              return;
            }
          }
          hasCheckedResume = true;
        } catch (error: any) {
          logger.error(`[RESUME] Failed to check resume block: ${error?.message}`);
          hasCheckedResume = true;
        }
      }

      const envCheck = validateApiVariables();
      if (!envCheck.isValid) {
        logger.error(`[CONFIG] Missing env vars: ${envCheck.missingVars.join(", ")}`);
        throw new Error(`Missing required environment variables: ${envCheck.missingVars.join(", ")}`);
      }

      const eventsCount = block.events.length;
      if (eventsCount > 0) {
        logger.info(`[BLOCK] #${block.header.blockNumber} — ${eventsCount} event(s)`);
      }

      for (const event of block.events) {
        const rawSel = event.keys?.[0];
        const selector = rawSel ? pad64(rawSel) : "unknown";
        const eventName = SELECTOR_TO_NAME[selector] ?? `unknown(${selector})`;
        const txShort = event.transactionHash?.slice(0, 12) + "...";

        try {
          const { db: storageDb } = useDrizzleStorage();

          // CommitmentAdded
          if (isEventType(event, EVENT_KEYS.CommitmentAdded)) {
            logger.info(`[EVENT] CommitmentAdded tx=${txShort} block=${block.header.blockNumber}`);

            const decoded = decodeCommitmentAdded(event, logger);
            if (!decoded) continue;

            const existing = await storageDb
              .select()
              .from(commitments)
              .where(eq(commitments.transactionHash, event.transactionHash))
              .limit(1);

            if (existing.length > 0) {
              logger.info(`[SKIP] CommitmentAdded already indexed tx=${txShort}`);
              continue;
            }

            await storageDb.insert(commitments).values({
              eventId: event.transactionHash,
              transactionHash: event.transactionHash,
              commitment: decoded.commitment,
              timestamp: new Date(),
              blockNumber: Number(block.header.blockNumber),
            });
            logger.info(`[DB] CommitmentAdded inserted commitment=${decoded.commitment}`);

            const forwarded = await forwardToRelayer(
              "commitment_added",
              { commitment: decoded.commitment },
              event.transactionHash,
              logger,
              Number(block.header.blockNumber),
            );
            if (!forwarded) logger.warn(`[RELAY] CommitmentAdded forward failed tx=${txShort}`);
          }

          // IntentSettled
          else if (isEventType(event, EVENT_KEYS.IntentSettled)) {
            logger.info(`[EVENT] IntentSettled tx=${txShort} block=${block.header.blockNumber}`);

            const decoded = decodeIntentSettled(event, logger);
            if (!decoded) continue;

            const existing = await storageDb
              .select()
              .from(intents_settled)
              .where(eq(intents_settled.transactionHash, event.transactionHash))
              .limit(1);

            if (existing.length > 0) {
              logger.info(`[SKIP] IntentSettled already indexed tx=${txShort}`);
              continue;
            }

            const amountLow = (decoded.amount & BigInt("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")).toString();
            const amountHigh = (decoded.amount >> BigInt(128)).toString();

            await storageDb.insert(intents_settled).values({
              eventId: event.transactionHash,
              transactionHash: event.transactionHash,
              intentId: decoded.intent_id,
              nullifierHash: decoded.nullifier_hash,
              token: decoded.token,
              amountLow,
              amountHigh,
              eventTimestamp: Number(decoded.timestamp),
              timestamp: new Date(),
              blockNumber: Number(block.header.blockNumber),
            });
            logger.info(`[DB] IntentSettled inserted intent_id=${decoded.intent_id}`);

            const forwarded = await forwardToRelayer(
              "settled",
              {
                intent_id: decoded.intent_id,
                nullifier_hash: decoded.nullifier_hash,
                token: decoded.token,
                amount: decoded.amount.toString(),
                event_timestamp: decoded.timestamp.toString(),
              },
              event.transactionHash,
              logger,
              Number(block.header.blockNumber),
            );
            if (!forwarded) logger.warn(`[RELAY] IntentSettled forward failed tx=${txShort}`);
          }

          // IntentMarkedSettled
          else if (isEventType(event, EVENT_KEYS.IntentMarkedSettled)) {
            logger.info(`[EVENT] IntentMarkedSettled tx=${txShort} block=${block.header.blockNumber}`);

            const decoded = decodeIntentMarkedSettled(event, logger);
            if (!decoded) continue;

            const existing = await storageDb
              .select()
              .from(intents_marked_settled)
              .where(eq(intents_marked_settled.transactionHash, event.transactionHash))
              .limit(1);

            if (existing.length > 0) {
              logger.info(`[SKIP] IntentMarkedSettled already indexed tx=${txShort}`);
              continue;
            }

            await storageDb.insert(intents_marked_settled).values({
              eventId: event.transactionHash,
              transactionHash: event.transactionHash,
              nullifierHash: decoded.nullifier_hash,
              commitment: decoded.commitment,
              eventTimestamp: Number(decoded.timestamp),
              timestamp: new Date(),
              blockNumber: Number(block.header.blockNumber),
            });
            logger.info(`[DB] IntentMarkedSettled inserted nullifier=${decoded.nullifier_hash}`);

            const forwarded = await forwardToRelayer(
              "marked_settled",
              {
                nullifier_hash: decoded.nullifier_hash,
                commitment: decoded.commitment,
                event_timestamp: decoded.timestamp.toString(),
              },
              event.transactionHash,
              logger,
              Number(block.header.blockNumber),
            );
            if (!forwarded) logger.warn(`[RELAY] IntentMarkedSettled forward failed tx=${txShort}`);
          }

          // MerkleRootUpdated
          else if (isEventType(event, EVENT_KEYS.MerkleRootUpdated)) {
            logger.info(`[EVENT] MerkleRootUpdated tx=${txShort} block=${block.header.blockNumber}`);

            const decoded = decodeMerkleRootUpdated(event, logger);
            if (!decoded) continue;

            const existing = await storageDb
              .select()
              .from(merkle_roots)
              .where(eq(merkle_roots.transactionHash, event.transactionHash))
              .limit(1);

            if (existing.length > 0) {
              logger.info(`[SKIP] MerkleRootUpdated already indexed tx=${txShort}`);
              continue;
            }

            await storageDb.insert(merkle_roots).values({
              eventId: event.transactionHash,
              transactionHash: event.transactionHash,
              newRoot: decoded.new_root,
              timestamp: new Date(),
              blockNumber: Number(block.header.blockNumber),
            });
            logger.info(`[DB] MerkleRootUpdated inserted new_root=${decoded.new_root}`);

            const forwarded = await forwardToRelayer(
              "merkle_root_updated",
              { new_root: decoded.new_root },
              event.transactionHash,
              logger,
              Number(block.header.blockNumber),
            );
            if (!forwarded) logger.warn(`[RELAY] MerkleRootUpdated forward failed tx=${txShort}`);
          }

          // BatchProcessed
          else if (isEventType(event, EVENT_KEYS.BatchProcessed)) {
            logger.info(`[EVENT] BatchProcessed tx=${txShort} block=${block.header.blockNumber}`);

            const decoded = decodeBatchProcessed(event, logger);
            if (!decoded) continue;

            const existing = await storageDb
              .select()
              .from(batches_processed)
              .where(eq(batches_processed.transactionHash, event.transactionHash))
              .limit(1);

            if (existing.length > 0) {
              logger.info(`[SKIP] BatchProcessed already indexed tx=${txShort}`);
              continue;
            }

            await storageDb.insert(batches_processed).values({
              eventId: event.transactionHash,
              transactionHash: event.transactionHash,
              batchId: Number(decoded.batch_id),
              commitmentsCount: Number(decoded.commitments_count),
              reason: decoded.reason,
              timestamp: new Date(),
              blockNumber: Number(block.header.blockNumber),
            });
            logger.info(`[DB] BatchProcessed inserted batch_id=${decoded.batch_id} count=${decoded.commitments_count}`);
          }

          // Unhandled — log selector so we can identify it
          else {
            logger.info(`[UNHANDLED] ${eventName} selector=${selector} tx=${txShort}`);
          }

        } catch (error: any) {
          logger.error(`[ERROR] Failed processing event type=${eventName} tx=${event.transactionHash}: ${error?.message}`, {
            selector,
            keys: event.keys,
            data: event.data,
            stack: error?.stack,
          });
        }
      }
    },
  });
}
