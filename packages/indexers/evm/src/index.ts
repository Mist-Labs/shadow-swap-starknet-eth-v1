import express, { NextFunction, Request, Response } from "express";
import { config } from "./config";
import { GoldskyWebhookPayload } from "./types";
import { transformGoldskyPayload } from "./handler";
import { forwardToRelayer } from "./relayer";

const SUPPORTED_ENTITIES = [
  "commitment_added",
  "intent_settled",
  "intent_marked_settled",
  "merkle_root_updated",
  "batch_processed",
];

const SUPPORTED_CHAINS = [
  "1",           // Ethereum mainnet
  "11155111",    // Sepolia
  "ethereum",
  "sepolia",
];

const app = express();

app.use(express.json());

// Webhook authentication middleware
app.use(
  "/webhook",
  (req: Request, _res: Response, next: NextFunction): void => {
    const secret = req.headers["goldsky-webhook-secret"];

    if (!secret || secret !== config.goldskyWebhookSecret) {
      console.warn("⚠️  Unauthorized webhook attempt");
      _res.status(401).json({ error: "Unauthorized" });
      return;
    }

    next();
  }
);

// In-memory processed events cache (simple deduplication)
const processedEvents = new Map<string, number>();
const CACHE_TTL = 3600000; // 1 hour

// Cleanup old cache entries every 10 minutes
setInterval(() => {
  const now = Date.now();
  for (const [key, timestamp] of processedEvents.entries()) {
    if (now - timestamp > CACHE_TTL) {
      processedEvents.delete(key);
    }
  }
}, 600000);

app.post("/webhook", async (req: Request, res: Response): Promise<void> => {
  try {
    const payload: GoldskyWebhookPayload = req.body;

    // Validate payload structure
    if (!payload.data?.new || !payload.entity) {
      console.error("❌ Invalid Goldsky payload structure");
      res.status(400).json({
        error: "Invalid webhook payload",
        details: "Missing data.new or entity field",
      });
      return;
    }

    const { entity, data } = payload;
    const eventData = data.new;

    // Create idempotency key
    const idempotencyKey = eventData.id || `${eventData.transaction_hash}-${eventData.log_index || 0}`;

    // Check if already processed
    if (processedEvents.has(idempotencyKey)) {
      console.log(`⏭️  Already processed: ${idempotencyKey}`);
      res.status(200).json({ status: "already_processed" });
      return;
    }

    console.log(`📦 Received: ${entity} | TX: ${eventData.transaction_hash?.substring(0, 10)}...`);

    // Validate entity type
    if (!SUPPORTED_ENTITIES.includes(entity)) {
      console.log(`⚠️  Unsupported entity: ${entity}`);
      res.status(200).json({ status: "ignored", reason: "unsupported_entity" });
      return;
    }

    // Respond immediately to Goldsky (don't make them wait)
    res.status(200).json({
      status: "received",
      idempotency_key: idempotencyKey,
    });

    // Process asynchronously (don't block response)
    processEvent(payload, idempotencyKey).catch((error) => {
      console.error(`❌ Async processing failed for ${idempotencyKey}:`, error);
    });
  } catch (error) {
    console.error("❌ Webhook error:", error);
    res.status(500).json({ error: "Internal server error" });
  }
});

/**
 * Process event asynchronously
 */
async function processEvent(payload: GoldskyWebhookPayload, idempotencyKey: string): Promise<void> {
  try {
    // Transform to relayer format
    const transformedPayload = await transformGoldskyPayload(payload);

    // Forward to relayer
    await forwardToRelayer(transformedPayload);

    // Mark as processed
    processedEvents.set(idempotencyKey, Date.now());

    console.log(`✅ Completed: ${payload.entity}`);
  } catch (error) {
    console.error(`❌ Processing failed for ${idempotencyKey}:`, error);
    // Don't throw - we already responded to Goldsky
  }
}

app.get("/health", (_req: Request, res: Response): void => {
  res.status(200).json({
    status: "healthy",
    timestamp: Date.now(),
    processed_events: processedEvents.size,
  });
});

const startServer = async () => {
  app.listen(config.port, () => {
    console.log(`🚀 ShadowSwap Indexer running on port ${config.port}`);
    console.log(`   Environment: ${config.nodeEnv}`);
    console.log(`   Relayer: ${config.relayerBaseUrl}`);
  });
};

startServer();
