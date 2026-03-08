import axios from "axios";
import { Pool } from "pg";
import crypto from "crypto";
import { ethers } from "ethers";

const GOLDSKY_URL = process.env.GOLDSKY_URL!;
const GOLDSKY_API_KEY = process.env.GOLDSKY_API_KEY!;
const DATABASE_URL = process.env.DATABASE_URL!;
const RELAYER_BASE_URL = process.env.RELAYER_BASE_URL!;
const HMAC_SECRET = process.env.HMAC_SECRET!;

const EVM_TREE_NAME = "evm_commitments";
const EVM_CHAIN_NAME = "ethereum";
const PAGE_SIZE = 1000;

const pool = new Pool({ connectionString: DATABASE_URL });

// ─── Goldsky ────────────────────────────────────────────────────────────────

async function fetchAllCommitmentsFromGoldsky(): Promise<any[]> {
  const all: any[] = [];
  let skip = 0;

  while (true) {
    const query = `
      query($skip: Int!, $first: Int!) {
        commitmentAddeds(
          first: $first
          skip: $skip
          orderBy: block_number
          orderDirection: asc
        ) {
          id
          commitment
          block_number
          transactionHash_
          contractId_
        }
      }
    `;

    const res = await axios.post(
      GOLDSKY_URL,
      { query, variables: { skip, first: PAGE_SIZE } },
      {
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${GOLDSKY_API_KEY}`,
        },
      }
    );

    const items = res.data?.data?.commitmentAddeds ?? [];
    console.log(`  Fetched ${items.length} commitments (skip=${skip})`);

    all.push(...items);

    if (items.length < PAGE_SIZE) break;
    skip += PAGE_SIZE;
  }

  return all;
}

// ─── DB helpers ─────────────────────────────────────────────────────────────

async function clearEvmData(client: any) {
  console.log("🗑️  Clearing EVM commitments and merkle data...");

  await client.query(
    `DELETE FROM merkle_leaves WHERE tree_name = $1`,
    [EVM_TREE_NAME]
  );
  await client.query(
    `DELETE FROM merkle_roots WHERE tree_name = $1`,
    [EVM_TREE_NAME]
  );
  await client.query(
    `DELETE FROM commitments WHERE chain = $1`,
    [EVM_CHAIN_NAME]
  );

  console.log("✅ EVM data cleared");
}

async function insertCommitment(client: any, item: any) {
  await client.query(
    `INSERT INTO commitments (chain, commitment_hash, intent_id, block_number, log_index)
     VALUES ($1, $2, $3, $4, $5)
     ON CONFLICT (commitment_hash) DO NOTHING`,
    [
      EVM_CHAIN_NAME,
      normalizeHex(item.commitment),
      item.contractId_ ? normalizeHex(item.contractId_) : null,
      parseInt(item.block_number),
      0,
    ]
  );
}

async function insertMerkleLeaf(client: any, leaf: string, index: number) {
  await client.query(
    `INSERT INTO merkle_leaves (tree_name, leaf, leaf_index)
     VALUES ($1, $2, $3)
     ON CONFLICT (tree_name, leaf_index) DO UPDATE SET leaf = EXCLUDED.leaf`,
    [EVM_TREE_NAME, leaf, index]
  );
}

async function insertMerkleRoot(client: any, root: string, leafCount: number) {
  await client.query(
    `INSERT INTO merkle_roots (tree_name, root, leaf_count) VALUES ($1, $2, $3)`,
    [EVM_TREE_NAME, root, leafCount]
  );
}

// ─── Merkle (incremental, matches on-chain) ─────────────────────────────────

const TREE_HEIGHT = 20;
const ZERO_LEAF = "0x0000000000000000000000000000000000000000000000000000000000000000";

function keccak256Pair(left: string, right: string): string {
  const l = Buffer.from(left.replace("0x", ""), "hex");
  const r = Buffer.from(right.replace("0x", ""), "hex");
  const lBig = BigInt("0x" + l.toString("hex"));
  const rBig = BigInt("0x" + r.toString("hex"));
  const [first, second] = lBig < rBig ? [l, r] : [r, l];
  return ethers.keccak256(ethers.concat([first, second]));
}

function computeZeros(): string[] {
  const zeros: string[] = [ZERO_LEAF];
  for (let i = 1; i < TREE_HEIGHT; i++) {
    zeros.push(keccak256Pair(zeros[i - 1], zeros[i - 1]));
  }
  return zeros;
}

function computeRootIncremental(leaves: string[]): string {
  if (leaves.length === 0) return ZERO_LEAF;

  const zeros = computeZeros();
  const filledSubtrees: string[] = new Array(TREE_HEIGHT).fill(ZERO_LEAF);
  let currentRoot = ZERO_LEAF;

  for (let leafIndex = 0; leafIndex < leaves.length; leafIndex++) {
    let currentHash = leaves[leafIndex];
    let index = leafIndex;

    for (let height = 0; height < TREE_HEIGHT; height++) {
      let left: string;
      let right: string;

      if ((index & 1) === 0) {
        filledSubtrees[height] = currentHash;
        left = currentHash;
        right = zeros[height];
      } else {
        left = filledSubtrees[height];
        right = currentHash;
      }

      currentHash = keccak256Pair(left, right);
      index >>= 1;
    }

    currentRoot = currentHash;
  }

  return currentRoot;
}

// ─── Relayer retry ───────────────────────────────────────────────────────────

function generateHMAC(payload: any): { signature: string; timestamp: string } {
  const body = JSON.stringify(payload);
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const signature = crypto
    .createHmac("sha256", HMAC_SECRET)
    .update(timestamp + body)
    .digest("hex");
  return { signature, timestamp };
}

async function retryFailedSettlements(client: any) {
  console.log("\n🔄 Retrying failed settlements...");

  const { rows } = await client.query(`
    SELECT id, commitment, nullifier_hash, source_chain
    FROM shadow_intents
    WHERE status = 'settlement_failed'
       OR status = 'pending'
    ORDER BY created_at ASC
  `);

  console.log(`  Found ${rows.length} intents to retry`);

  for (const intent of rows) {
    const payload = {
      event_type: "retry_settlement",
      chain: intent.source_chain,
      intent_id: intent.id,
      commitment: intent.commitment,
      nullifier_hash: intent.nullifier_hash,
    };

    const { signature, timestamp } = generateHMAC(payload);

    try {
      await axios.post(`${RELAYER_BASE_URL}/indexer/event`, payload, {
        headers: {
          "Content-Type": "application/json",
          "x-signature": signature,
          "x-timestamp": timestamp,
        },
        timeout: 30000,
      });

      await client.query(
        `UPDATE shadow_intents SET status = 'pending', updated_at = $1 WHERE id = $2`,
        [Date.now(), intent.id]
      );

      console.log(`  ✅ Retried intent ${intent.id.substring(0, 10)}...`);
    } catch (err: any) {
      console.error(`  ❌ Failed to retry ${intent.id.substring(0, 10)}: ${err.message}`);
    }
  }
}

// ─── Utils ───────────────────────────────────────────────────────────────────

function normalizeHex(val: string): string {
  if (!val) return val;
  return val.startsWith("0x") ? val.toLowerCase() : `0x${val.toLowerCase()}`;
}

// ─── Main ────────────────────────────────────────────────────────────────────

async function reindex() {
  const client = await pool.connect();

  try {
    await client.query("BEGIN");

    await clearEvmData(client);

    console.log(`\n📡 Fetching commitments from Goldsky...`);
    const items = await fetchAllCommitmentsFromGoldsky();
    console.log(`  Total: ${items.length} commitments (including duplicates)`);

    if (items.length === 0) {
      console.log("  No commitments found, nothing to do");
      await client.query("COMMIT");
      return;
    }

    const leaves: string[] = [];

    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      const leaf = normalizeHex(item.commitment);

      await insertCommitment(client, item);
      await insertMerkleLeaf(client, leaf, i);
      leaves.push(leaf);
    }

    console.log(`\n🌳 Recomputing Merkle root with ${leaves.length} leaves...`);
    const root = computeRootIncremental(leaves);
    await insertMerkleRoot(client, root, leaves.length);
    console.log(`✅ New root: ${root}`);
    console.log(`   Expected: 0x798b3c9370d30db6d9f5dbab78b72fefd442910baca0070c8eace0a6585f32ff`);
    console.log(`   Match: ${root.toLowerCase() === "0x798b3c9370d30db6d9f5dbab78b72fefd442910baca0070c8eace0a6585f32ff"}`);

    await client.query("COMMIT");
    console.log("\n✅ Reindex committed to DB");

    await retryFailedSettlements(client);

  } catch (err) {
    await client.query("ROLLBACK");
    console.error("❌ Reindex failed, rolled back:", err);
    throw err;
  } finally {
    client.release();
    await pool.end();
  }
}

reindex().catch((err) => {
  console.error(err);
  process.exit(1);
});