import { keccak256 } from "js-sha3"
import { hash } from "starknet"
import { encrypt } from "eciesjs"
import { hexToBytes, toHex } from "viem"
import { RELAYER_PUBLIC_KEY } from "./contracts"

// Constants
const DOMAIN = "ShadowSwap.ViewKey.v1"

/**
 * Encrypt raw bytes (secret or nullifier) using ECIES
 */
export function eciesEncryptBytes(hexValue: string): string {
    if (!RELAYER_PUBLIC_KEY) throw new Error("Missing relayer public key")
    const plaintext = Buffer.from(hexValue.replace("0x", ""), "hex")
    const pubKey = Buffer.from(RELAYER_PUBLIC_KEY, "hex")
    return "0x" + encrypt(pubKey, plaintext).toString("hex")
}

/**
 * Encrypt UTF-8 string (recipient address) using ECIES
 */
export function eciesEncryptString(value: string): string {
    if (!RELAYER_PUBLIC_KEY) throw new Error("Missing relayer public key")
    const plaintext = Buffer.from(value, "utf8")
    const pubKey = Buffer.from(RELAYER_PUBLIC_KEY, "hex")
    return "0x" + encrypt(pubKey, plaintext).toString("hex")
}

/**
 * Utility to zero-pad a bytes32 buffer from a BigInt
 */
function bigintToBytes32(value: bigint): Uint8Array {
    let hex = value.toString(16)
    if (hex.length % 2 !== 0) hex = "0" + hex
    hex = hex.padStart(64, "0")
    return hexToBytes(("0x" + hex) as `0x${string}`)
}

/**
 * Concatenate multiple Uint8Arrays
 */
function concat(arrays: Uint8Array[]): Uint8Array {
    let totalLength = 0;
    for (const arr of arrays) {
        totalLength += arr.length;
    }
    const result = new Uint8Array(totalLength);
    let offset = 0;
    for (const arr of arrays) {
        result.set(arr, offset);
        offset += arr.length;
    }
    return result;
}

export interface PrivacyParams {
    _secret: string
    _nullifier: string
    commitment: string
    nullifier_hash: string
}

/**
 * Generate 5-parameter EVM Privacy Params
 */
export function generateEvmPrivacyParams({
    amount,
    token,
    destChain,
}: {
    amount: string
    token: string
    destChain: string
}): PrivacyParams {
    const DEST_CHAIN_ID = destChain === "evm" || destChain === "ethereum" ? BigInt(1) : BigInt(2)

    const secretBytes = crypto.getRandomValues(new Uint8Array(32))
    const nullifierBytes = crypto.getRandomValues(new Uint8Array(32))

    const _secret = "0x" + toHex(secretBytes).replace("0x", "")
    const _nullifier = "0x" + toHex(nullifierBytes).replace("0x", "")

    const tokenBytes = hexToBytes(("0x" + token.replace("0x", "").padStart(40, "0")) as `0x${string}`)
    const amountBytes = bigintToBytes32(BigInt(amount))
    const destBytes = bigintToBytes32(DEST_CHAIN_ID)

    const preimage = concat([secretBytes, nullifierBytes, amountBytes, tokenBytes, destBytes])

    const commitment = "0x" + keccak256(preimage)
    const nullifier_hash = "0x" + keccak256(nullifierBytes)

    return { _secret, _nullifier, commitment, nullifier_hash }
}

/**
 * Generate 5-parameter Starknet Privacy Params
 */
export function generateStarknetPrivacyParams({
    amount,
    token,
    destChain,
}: {
    amount: string
    token: string
    destChain: string
}): PrivacyParams {
    const DEST_CHAIN_ID = destChain === "evm" || destChain === "ethereum" ? BigInt(1) : BigInt(2)

    // 31 bytes to stay below field prime
    const secretBytes = crypto.getRandomValues(new Uint8Array(31))
    const nullifierBytes = crypto.getRandomValues(new Uint8Array(31))

    const _secret = "0x" + toHex(secretBytes).replace("0x", "")
    const _nullifier = "0x" + toHex(nullifierBytes).replace("0x", "")

    const secretFelt = BigInt(_secret)
    const nullifierFelt = BigInt(_nullifier)
    const tokenFelt = BigInt(token)
    
    // Split u256 amount into low/high 128-bit values for Cairo compatibility
    const amountBI = BigInt(amount)
    const mask128 = (BigInt(1) << BigInt(128)) - BigInt(1)
    const amountLow = amountBI & mask128
    const amountHigh = amountBI >> BigInt(128)

    // commitment = poseidon_hash_many([secret, nullifier, amount_low, amount_high, token, destChain])
    const commitment =
        "0x" +
        hash.computePoseidonHashOnElements([
            secretFelt,
            nullifierFelt,
            amountLow,
            amountHigh,
            tokenFelt,
            DEST_CHAIN_ID
        ])
            .replace("0x", "")
            .padStart(64, "0")

    const nullifier_hash =
        "0x" +
        hash.computePoseidonHashOnElements([nullifierFelt])
            .replace("0x", "")
            .padStart(64, "0")

    return { _secret, _nullifier, commitment, nullifier_hash }
}

/**
 * Derive deterministic view key
 */
export function deriveViewKey(walletAddress: string, sourceChain: string): string {
    if (sourceChain === "evm" || sourceChain === "ethereum") {
        return "0x" + keccak256(walletAddress.toLowerCase() + DOMAIN)
    } else {
        const addrFelt = BigInt(walletAddress)
        const domainFelt = BigInt("0x" + Buffer.from(DOMAIN).toString("hex"))
        return "0x" + hash.computePoseidonHashOnElements([addrFelt, domainFelt]).replace("0x", "").padStart(64, "0")
    }
}
