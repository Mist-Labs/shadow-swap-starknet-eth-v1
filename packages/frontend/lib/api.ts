/**
 * API Client for Starknet-Ethereum Privacy Bridge Backend
 *
 * Handles all communication with the backend relayer API including:
 * - HMAC authentication
 * - Bridge intent initiation
 * - Intent status polling
 * - Price feed queries
 * - Automatic retry on network failures
 */

import type { ChainType } from "./tokens";
export async function retryAPICall<T>(
  fn: () => Promise<T>,
  retries: number = 3,
  delay: number = 1000
): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    if (retries <= 0) throw error;
    await new Promise((resolve) => setTimeout(resolve, delay));
    return retryAPICall(fn, retries - 1, delay * 2);
  }
}
import {
  APIError,
  HMACAuthError,
  IntentNotFoundError,
  NetworkTimeoutError,
} from "./errors";

/**
 * API Configuration
 */
const API_BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL || "";

const HMAC_SECRET = process.env.NEXT_PUBLIC_HMAC_SECRET || "";

/**
 * Intent status types
 */
export type IntentStatus =
  | "created"
  | "committed"
  | "filled"
  | "completed"
  | "refunded"
  | "failed";

/**
 * Bridge intent initiation request
 * Updated to match API v1.0.0 specification
 */
export interface BridgeInitiateRequest {
  intent_id: string;
  user_address: string;
  source_chain: ChainType;
  dest_chain: ChainType;
  source_token: string;
  dest_token: string;
  amount: string;
  commitment: string;
  encrypted_secret: string; // ECIES encrypted secret
  encrypted_nullifier: string; // ECIES encrypted nullifier
  nullifier_hash: string;
  claim_auth: string;
  encrypted_recipient: string; // ECIES UTF-8 string
  refund_address: string;
  near_intents_id: string; // UUID from correlationId
  view_key: string;
  deposit_address: string;
}

/**
 * Bridge intent initiation response
 */
export interface BridgeInitiateResponse {
  success: boolean;
  intent_id: string;
  commitment: string;
  message: string;
  error: string | null;
}

/**
 * Backend raw response format.
 * Matches the Rust IntentStatusResponse struct in backend/src/api/model.rs exactly.
 */
interface BackendIntentResponse {
  intent_id: string;
  status: string;        // e.g. "pending", "batched", "settled"
  source_chain: string;  // "ethereum" | "starknet"
  dest_chain: string;
  token: string;         // single token field (not source_token/dest_token)
  amount: string;
  commitment: string;
  deposit_address: string | null;
  near_status: string | null;
  dest_tx_hash: string | null;
  settle_tx_hash: string | null;
  source_settle_tx_hash: string | null;
  created_at: number;    // unix timestamp (u64 in Rust)
  updated_at: number;
}

/**
 * Normalized intent status response (what frontend components use)
 */
export interface IntentStatusResponse {
  intent_id: string;
  status: IntentStatus;
  source_chain: string;
  dest_chain: string;
  source_token: string;  // same as token — kept for display compatibility
  dest_token: string;
  amount: string;
  commitment: string;
  deposit_address: string | null;
  near_status: string | null;
  dest_tx_hash: string | null;
  settle_tx_hash: string | null;
  source_settle_tx_hash: string | null;
  created_at: number;
  updated_at: number;
  has_privacy: boolean;
}

/**
 * Map backend v2.0 status strings → frontend display status
 */
function normalizeIntentResponse(
  backend: BackendIntentResponse
): IntentStatusResponse {
  const statusMap: Record<string, IntentStatus> = {
    pending:         "created",
    batched:         "committed",
    near_submitted:  "committed",
    tokens_delivered: "filled",
    settled:         "completed",
    marked_settled:  "completed",
    failed:          "failed",
    refunded:        "refunded",
  };

  return {
    intent_id:             backend.intent_id,
    status:                statusMap[backend.status] ?? "created",
    source_chain:          backend.source_chain,
    dest_chain:            backend.dest_chain,
    source_token:          backend.token,  // backend has single token field
    dest_token:            backend.token,
    amount:                backend.amount,
    commitment:            backend.commitment,
    deposit_address:       backend.deposit_address,
    near_status:           backend.near_status,
    dest_tx_hash:          backend.dest_tx_hash,
    settle_tx_hash:        backend.settle_tx_hash,
    source_settle_tx_hash: backend.source_settle_tx_hash,
    created_at:            backend.created_at,
    updated_at:            backend.updated_at,
    has_privacy:           true, // All ShadowSwap txs are privacy-preserving
  };
}

/**
 * Price query response
 */
export interface PriceResponse {
  from_symbol: string;
  to_symbol: string;
  rate: number;
  amount: number;
  converted_amount: number;
  timestamp: number;
  sources: Array<{
    source: string;
    price: number;
  }>;
}

/**
 * Initiate a bridge transaction
 *
 * @param request - Bridge initiation request
 * @returns Promise<BridgeInitiateResponse>
 */
export async function initiateBridge(
  request: BridgeInitiateRequest
): Promise<BridgeInitiateResponse> {
  return retryAPICall(async () => {
    try {
      const response = await fetch("/api/bridge", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request),
      });

      if (!response.ok) {
        if (response.status === 401) {
          throw new HMACAuthError();
        }

        const error = await response.json().catch(() => ({
          message: "Failed to initiate bridge",
        }));
        throw new APIError(
          response.status,
          error.message || error.error || "Failed to initiate bridge"
        );
      }

      return response.json();
    } catch (error) {
      if (error instanceof HMACAuthError || error instanceof APIError) {
        throw error;
      }
      throw new NetworkTimeoutError(error instanceof Error ? error : undefined);
    }
  }, 3);
}

/**
 * Get intent status by ID
 *
 * @param intentId - Intent ID
 * @returns Promise<IntentStatusResponse>
 */
export async function getIntentStatus(
  intentId: string
): Promise<IntentStatusResponse> {
  return retryAPICall(async () => {
    try {
      // Route through local proxy to keep backend URL server-side
      const response = await fetch(`/api/intent/${encodeURIComponent(intentId)}`);

      if (!response.ok) {
        if (response.status === 404) {
          throw new IntentNotFoundError(intentId);
        }
        throw new APIError(response.status, "Failed to fetch intent status");
      }

      const backendData: BackendIntentResponse = await response.json();
      return normalizeIntentResponse(backendData);
    } catch (error) {
      if (error instanceof IntentNotFoundError || error instanceof APIError) {
        throw error;
      }
      throw new NetworkTimeoutError(error instanceof Error ? error : undefined);
    }
  }, 3);
}

/**
 * Poll intent status until it reaches a terminal state
 *
 * @param intentId - Intent ID to poll
 * @param onUpdate - Callback for status updates
 * @param maxAttempts - Maximum polling attempts (default: 60)
 * @param intervalMs - Polling interval in ms (default: 5000)
 * @returns Promise<IntentStatusResponse> - Returns last known status even if terminal state not reached
 */
export async function pollIntentStatus(
  intentId: string,
  onUpdate?: (status: IntentStatusResponse) => void,
  maxAttempts: number = 60,
  intervalMs: number = 5000
): Promise<IntentStatusResponse> {
  const terminalStates: IntentStatus[] = ["completed", "refunded", "failed"];
  let lastStatus: IntentStatusResponse | null = null;

  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    const status = await getIntentStatus(intentId);
    lastStatus = status;

    // Call update callback
    if (onUpdate) {
      onUpdate(status);
    }

    // Check if terminal state reached
    if (terminalStates.includes(status.status)) {
      return status;
    }

    // Wait before next poll
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }

  // If we reach here, polling timed out but transaction is still valid
  // Return the last known status instead of throwing an error
  console.log("✅ Polling completed without reaching terminal state. Transaction is still processing.");
  return lastStatus!;
}

/**
 * Get exchange rate between two tokens
 *
 * @param fromSymbol - Source token symbol
 * @param toSymbol - Destination token symbol
 * @param amount - Amount to convert (optional)
 * @returns Promise<PriceResponse>
 */
/**
 * Get exchange rate between two tokens.
 *
 * NOTE: No /price endpoint exists in the current backend.
 * Rate is fetched client-side from a public price oracle or hardcoded.
 * This function is a stub until the backend adds a pricing endpoint.
 */
export async function getExchangeRate(
  _fromSymbol: string,
  _toSymbol: string,
  _amount?: number
): Promise<PriceResponse> {
  // Hardcoded 1:1 stub — replace with a real price oracle call when available
  return {
    from_symbol: _fromSymbol,
    to_symbol: _toSymbol,
    rate: 1,
    amount: _amount ?? 1,
    converted_amount: _amount ?? 1,
    timestamp: Date.now(),
    sources: [],
  };
}

/**
 * Health check endpoint
 *
 * @returns Promise<{ status: string; components: Record<string, string> }>
 */
export async function healthCheck(): Promise<{
  status: string;
  timestamp: string;
  components: Record<string, string>;
}> {
  // Always use the local proxy — API_BASE_URL is not available in browser bundles
  const response = await fetch(`/api/health`);
  if (!response.ok) {
    throw new Error("Health check failed");
  }
  return response.json();
}

/**
 * Get bridge statistics
 *
 * @returns Promise<Record<string, unknown>>
 */
export async function getBridgeStats(): Promise<{
  status: string;
  data: {
    // Matches the backend MetricsData struct returned by /metrics
    total_intents: number;
    pending_intents: number;
    settled_intents: number;
    failed_intents: number;
    refunded_intents: number;
    // These fields may not exist yet - treat as optional
    filled_intents?: number;
    completed_intents?: number;
    ethereum_to_starknet?: number;
    starknet_to_ethereum?: number;
    total_volume_by_token?: Record<string, string>;
  };
}> {
  // Always use the local proxy — API_BASE_URL is not available in browser bundles
  // Proxy hits /metrics on the backend (HMAC-signed server-side)
  const response = await fetch(`/api/stats`);
  if (!response.ok) {
    throw new Error("Failed to fetch bridge stats");
  }
  return response.json();
}


/**
 * List bridge intents with optional filters
 *
 * @param options - Filter options
 * @returns Promise<IntentStatusResponse[]>
 */
export async function listBridgeIntents(options?: {
  status?: IntentStatus;
  chain?: ChainType;
  limit?: number;
  userAddress?: string;
  viewKey?: string;
}): Promise<{
  status: string;
  count: number;
  data: IntentStatusResponse[];
}> {
  const params = new URLSearchParams();

  if (options?.status) params.append("status", options.status);
  if (options?.chain) params.append("chain", options.chain);
  if (options?.limit) params.append("limit", options.limit.toString());
  if (options?.userAddress) params.append("user_address", options.userAddress);
  // Forward view_key so the proxy can pass it to the backend
  // (backend will match intents stored with this view key)
  if (options?.viewKey) params.append("view_key", options.viewKey);

  const queryString = params.toString();

  // Always call through the local Next.js proxy (/api/intents).
  const url = `/api/intents${queryString ? `?${queryString}` : ""}`;

  const response = await fetch(url);

  if (!response.ok) {
    throw new Error("Failed to fetch bridge intents");
  }

  const body: {
    status: string;
    count: number;
    data: IntentStatusResponse[] | BackendIntentResponse[];
  } = await response.json();

  if (!body.data || body.data.length === 0) {
    return { status: body.status ?? "success", count: 0, data: [] };
  }

  const rawData = body.data as BackendIntentResponse[];
  return {
    status: body.status,
    count: body.count,
    data: rawData.map(normalizeIntentResponse),
  };
}

/**
 * Get all token prices in USD
 *
 * @returns Promise<PricesAllResponse>
 */
export async function getAllPrices(): Promise<{
  status: string;
  timestamp: number;
  prices: Record<string, number>;
}> {
  const response = await fetch(`${API_BASE_URL}/prices/all`);

  if (!response.ok) {
    throw new Error("Failed to fetch all prices");
  }

  return response.json();
}

/**
 * Convert amount between tokens
 *
 * @param fromSymbol - Source token symbol
 * @param toSymbol - Destination token symbol
 * @param amount - Amount to convert
 * @returns Promise<ConvertResponse>
 */
export async function convertAmount(
  fromSymbol: string,
  toSymbol: string,
  amount: number
): Promise<{
  from_symbol: string;
  to_symbol: string;
  input_amount: number;
  output_amount: number;
  rate: number;
  timestamp: number;
}> {
  const response = await fetch(`${API_BASE_URL}/price/convert`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      from_symbol: fromSymbol,
      to_symbol: toSymbol,
      amount,
    }),
  });

  if (!response.ok) {
    throw new Error("Failed to convert amount");
  }

  return response.json();
}

/**
 * Get API service information
 *
 * @returns Promise<ServiceInfo>
 */
export async function getServiceInfo(): Promise<{
  service: string;
  version: string;
  status: string;
  supported_chains: string[];
  supported_tokens: string[];
}> {
  const response = await fetch(`${API_BASE_URL}/`);

  if (!response.ok) {
    throw new Error("Failed to fetch service info");
  }

  return response.json();
}

/**
 * Validate API configuration
 */
export function validateAPIConfig(): void {
  if (!API_BASE_URL) {
    throw new Error("API base URL not configured");
  }
  if (!HMAC_SECRET) {
    console.warn(
      "HMAC secret not configured. Bridge transactions will fail. Please set NEXT_PUBLIC_HMAC_SECRET."
    );
  }
}
