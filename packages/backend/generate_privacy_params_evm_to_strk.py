#!/usr/bin/env python3
"""
Generate Privacy Parameters for ShadowSwap + get NEAR deposit address.
Direction: EVM (ETH/USDC/USDT) → StarkNet (STRK/ETH)

Steps:
  1. Generates Keccak256 commitment + ECIES-encrypted params (EVM source chain)
  2. Calls NEAR 1Click /v0/quote → gets deposit_address + correlationId
  3. Posts to /api/v1/bridge/initiate

Hash: Keccak256 (EVM source) — NOT Poseidon.
near_intents_id = correlationId from NEAR (UUID, 128-bit, fits felt252).
NEVER use a random 32-byte hex for near_intents_id — it overflows felt252.
"""

import os, sys, secrets, json, hmac as hmaclib, hashlib, time, requests
from datetime import datetime, timezone, timedelta
from ecies import encrypt
from eth_hash.auto import keccak

# ── Load env ─────────────────────────────────────────────────────────────────
with open('.env') as f:
    for line in f:
        line = line.strip()
        if line and not line.startswith('#') and '=' in line:
            k, v = line.split('=', 1)
            os.environ[k] = v

NEAR_API_KEY       = os.environ['NEAR_API_KEY']
RELAYER_PUBLIC_KEY = os.environ['RELAYER_PUBLIC_KEY']
SN_SETTLEMENT      = os.environ['STARKNET_CONTRACT_ADDRESS']   # NEAR delivers here
EVM_ACCOUNT        = os.environ['EVM_ACCOUNT_ADDRESS']         # refundTo on failed swap
HMAC_SECRET        = os.environ['HMAC_SECRET']
BACKEND            = "http://127.0.0.1:8080"

# ── Swap parameters (edit as needed) ─────────────────────────────────────────
# Source: EVM USDT (6 decimals). Destination: StarkNet STRK.
AMOUNT_USDT   = "300000"   # 2 USDT (6 decimals)
USDT_TOKEN    = "0xdAC17F958D2ee523a2206206994597C13D831ec7"
RECIPIENT_SN  = os.environ['STARKNET_ACCOUNT_ADDRESS']  # StarkNet recipient

# NEAR asset IDs for EVM → StarkNet direction
NEAR_ORIGIN_ASSET = "nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near"
NEAR_DEST_ASSET   = "nep141:starknet.omft.near"

# dest_chain felt: 2 = StarkNet (must match contract enum — adjust if yours differs)
DEST_CHAIN_FELT = 2

print("=" * 70)
print("SHADOWSWAP — INITIATE BRIDGE (EVM USDT → StarkNet STRK)")
print("=" * 70)

# ── 1. Privacy params (Keccak256 for EVM source) ─────────────────────────────
secret    = secrets.token_hex(32)
nullifier = secrets.token_hex(32)

secret_bytes    = bytes.fromhex(secret)
nullifier_bytes = bytes.fromhex(nullifier)

# EVM commitment: keccak256(secret || nullifier || amount || token || destChain)
# Matches EVM contract: keccak256(abi.encodePacked(secret, nullifier, amount, token, destChain))
amount_int    = int(AMOUNT_USDT)
token_int     = int(USDT_TOKEN, 16)
dest_int      = DEST_CHAIN_FELT

commitment_preimage = (
    secret_bytes
    + nullifier_bytes
    + amount_int.to_bytes(32, 'big')
    + token_int.to_bytes(32, 'big')
    + dest_int.to_bytes(32, 'big')
)
commitment_felt    = int.from_bytes(keccak(commitment_preimage), 'big')
nullifier_hash_int = int.from_bytes(keccak(nullifier_bytes), 'big')

commitment     = "0x" + format(commitment_felt,    '064x')
nullifier_hash = "0x" + format(nullifier_hash_int, '064x')

# ── ECIES encrypt privacy params ─────────────────────────────────────────────
pub_key_hex = RELAYER_PUBLIC_KEY
for pfx in ("0x04", "04"):
    if pub_key_hex.startswith(pfx):
        pub_key_hex = pub_key_hex[len(pfx):]
        break
pub_key_bytes = bytes.fromhex(pub_key_hex)

enc_secret    = "0x" + encrypt(pub_key_bytes, bytes.fromhex(secret)).hex()
enc_nullifier = "0x" + encrypt(pub_key_bytes, bytes.fromhex(nullifier)).hex()
enc_recipient = "0x" + encrypt(pub_key_bytes, RECIPIENT_SN.lower().encode()).hex()

print(f"\ncommitment:     {commitment}")
print(f"nullifier_hash: {nullifier_hash}")

# ── 2. NEAR 1Click /v0/quote ─────────────────────────────────────────────────
deadline = (datetime.now(timezone.utc) + timedelta(minutes=30)).strftime('%Y-%m-%dT%H:%M:%SZ')
quote_body = {
    "dry": False,
    "swapType": "EXACT_INPUT",
    "slippageTolerance": 100,  # 1%
    "originAsset": NEAR_ORIGIN_ASSET,
    "destinationAsset": NEAR_DEST_ASSET,
    "amount": AMOUNT_USDT,
    "recipient": SN_SETTLEMENT,   # NEAR delivers to StarkNet settlement contract
    "refundTo": EVM_ACCOUNT,
    "depositType": "ORIGIN_CHAIN",
    "refundType": "ORIGIN_CHAIN",
    "recipientType": "DESTINATION_CHAIN",
    "referral": "shadowswap",
    "deadline": deadline,
}
print(f"\nCalling NEAR 1Click /v0/quote ...")
q = requests.post(
    "https://1click.chaindefuser.com/v0/quote",
    json=quote_body,
    headers={"Content-Type": "application/json", "Authorization": f"Bearer {NEAR_API_KEY}"},
    timeout=15,
)
if not q.ok:
    print(f"❌ NEAR quote failed {q.status_code}: {q.text}")
    sys.exit(1)

qd = q.json()
deposit_address = qd.get("quote", {}).get("depositAddress")
near_intents_id = qd.get("correlationId") or qd.get("correlation_id")
min_out = qd.get("quote", {}).get("minAmountOut", "?")

if not deposit_address:
    print("❌ No depositAddress in NEAR response:", json.dumps(qd, indent=2)[:800])
    sys.exit(1)
if not near_intents_id:
    print("❌ No correlationId in NEAR response:", json.dumps(qd, indent=2)[:800])
    sys.exit(1)

print(f"✅ deposit_address:  {deposit_address}")
print(f"   near_intents_id:  {near_intents_id}  (= NEAR correlationId)")
print(f"   min STRK out:     {min_out}")

# ── 3. POST /api/v1/bridge/initiate ──────────────────────────────────────────
intent_id = "0x" + secrets.token_hex(32)
view_key  = "0x" + secrets.token_hex(32)

payload = json.dumps({
    "intent_id":           intent_id,
    "commitment":          commitment,
    "nullifier_hash":      nullifier_hash,
    "view_key":            view_key,
    "near_intents_id":     near_intents_id,
    "source_chain":        "evm",
    "dest_chain":          "starknet",
    "encrypted_recipient": enc_recipient,
    "token":               USDT_TOKEN,
    "amount":              AMOUNT_USDT,
    "deposit_address":     deposit_address,
    "encrypted_secret":    enc_secret,
    "encrypted_nullifier": enc_nullifier,
}, separators=(',', ':'))

ts  = str(int(time.time()))
sig = hmaclib.new(HMAC_SECRET.encode(), (ts + payload).encode(), hashlib.sha256).hexdigest()

resp = requests.post(
    f"{BACKEND}/api/v1/bridge/initiate",
    data=payload,
    headers={"Content-Type": "application/json", "x-timestamp": ts, "x-signature": sig},
    timeout=10,
)

print(f"\nBackend ({resp.status_code}): {resp.text[:300]}")

if resp.ok:
    print(f"\n{'=' * 70}")
    print(f"🎯 Intent ID:       {intent_id}")
    print(f"📬 Send {int(AMOUNT_USDT)/1e6} USDT to: {deposit_address}")
    print(f"{'=' * 70}")
    print(f"\nPlaintext (keep secure):")
    print(f"  secret:    {secret}")
    print(f"  nullifier: {nullifier}")