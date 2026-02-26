#!/usr/bin/env python3
"""
ShadowSwap: 8 STRK (Starknet) → USDT (BSC)
originAsset:      nep141:starknet.omft.near
destinationAsset: nep245:v2_1.omni.hot.tg:56_2CMMyVTGZkeyNZTSvS5sarzfir6g
BSC USDT decimals: 18
"""

import os, sys, secrets, json, hmac as hmaclib, hashlib, time, requests
from datetime import datetime, timezone, timedelta
from ecies import encrypt
from poseidon_py.poseidon_hash import poseidon_hash_many

# ── Load env ──────────────────────────────────────────────────────────────────
with open('.env') as f:
    for line in f:
        line = line.strip()
        if line and not line.startswith('#') and '=' in line:
            k, v = line.split('=', 1)
            os.environ[k] = v

NEAR_API_KEY       = os.environ['NEAR_API_KEY']
RELAYER_PUBLIC_KEY = os.environ['RELAYER_PUBLIC_KEY']
EVM_SETTLEMENT     = os.environ['EVM_SETTLEMENT_ADDRESS']  # BSC settlement contract
SN_ACCOUNT         = os.environ['STARKNET_ACCOUNT_ADDRESS']
HMAC_SECRET        = os.environ['HMAC_SECRET']
BACKEND            = "http://127.0.0.1:8080"

# ── Swap parameters ───────────────────────────────────────────────────────────
AMOUNT_STRK    = "8000000000000000000"   # 8 STRK (18 decimals)
STRK_TOKEN     = "0x04718f5a0Fc34cC1AF16A1cdee98fFB20C31f5cD61D6Ab07201858f4287c938D"
RECIPIENT_BSC  = "0x2af423ba8cd60fe7ca0bbcc4cf1f4e6a7e576039"  # your BSC wallet

BSC_USDT_ASSET = "nep245:v2_1.omni.hot.tg:56_2CMMyVTGZkeyNZTSvS5sarzfir6g"

print("=" * 70)
print("SHADOWSWAP — StarkNet STRK → BSC USDT")
print("=" * 70)

# ── 1. Privacy params ─────────────────────────────────────────────────────────
secret    = secrets.token_hex(32)
nullifier = secrets.token_hex(32)

secret_felt = int(secret, 16)
null_felt   = int(nullifier, 16)
amount_felt = int(AMOUNT_STRK)
token_felt  = int(STRK_TOKEN, 16)
dest_felt   = 1  # EVM destination

nullifier_hash_felt = poseidon_hash_many([null_felt])
commitment_felt     = poseidon_hash_many([secret_felt, null_felt, amount_felt, token_felt, dest_felt])
nullifier_hash = "0x" + format(nullifier_hash_felt, '064x')
commitment     = "0x" + format(commitment_felt,     '064x')

pub_key_hex = RELAYER_PUBLIC_KEY
for pfx in ("0x04", "04"):
    if pub_key_hex.startswith(pfx):
        pub_key_hex = pub_key_hex[len(pfx):]
        break
pub_key_bytes = bytes.fromhex(pub_key_hex)

enc_secret    = "0x" + encrypt(pub_key_bytes, bytes.fromhex(secret)).hex()
enc_nullifier = "0x" + encrypt(pub_key_bytes, bytes.fromhex(nullifier)).hex()
enc_recipient = "0x" + encrypt(pub_key_bytes, RECIPIENT_BSC.lower().encode()).hex()

print(f"\ncommitment:     {commitment}")
print(f"nullifier_hash: {nullifier_hash}")

# ── 2. NEAR 1Click /v0/quote ──────────────────────────────────────────────────
deadline = (datetime.now(timezone.utc) + timedelta(minutes=30)).strftime('%Y-%m-%dT%H:%M:%SZ')
quote_body = {
    "dry": False,
    "swapType": "EXACT_INPUT",
    "slippageTolerance": 100,  # 1%
    "originAsset": "nep141:starknet.omft.near",   # STRK on Starknet
    "destinationAsset": BSC_USDT_ASSET,            # USDT on BSC
    "amount": AMOUNT_STRK,
    "recipient": EVM_SETTLEMENT,   # NEAR delivers to BSC settlement contract
    "refundTo": SN_ACCOUNT,
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
print(f"   near_intents_id:  {near_intents_id}")
print(f"   min USDT out:     {min_out}")

# ── 3. POST /api/v1/bridge/initiate ───────────────────────────────────────────
intent_id = "0x" + secrets.token_hex(32)
view_key  = "0x" + secrets.token_hex(32)

payload = json.dumps({
    "intent_id":           intent_id,
    "commitment":          commitment,
    "nullifier_hash":      nullifier_hash,
    "view_key":            view_key,
    "near_intents_id":     near_intents_id,
    "source_chain":        "starknet",
    "dest_chain":          "evm",
    "encrypted_recipient": enc_recipient,
    "token":               STRK_TOKEN,
    "amount":              AMOUNT_STRK,
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
    print(f"🎯 Intent ID:        {intent_id}")
    print(f"📬 Send 8 STRK to:   {deposit_address}")
    print(f"{'=' * 70}")
    print(f"\nPlaintext (keep secure):")
    print(f"  secret:    {secret}")
    print(f"  nullifier: {nullifier}")