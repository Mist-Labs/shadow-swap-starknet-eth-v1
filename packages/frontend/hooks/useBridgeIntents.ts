import { IntentStatusResponse, IntentStatus } from "@/lib/api"

export function useBridgeIntents(options?: { limit?: number; status?: IntentStatus; chain?: string }) {
  // Placeholder data, use options to satisfy linter
  const limit = options?.limit || 10;
  const statusFilter = options?.status;
  const chainFilter = options?.chain;
  
  const rawIntents: IntentStatusResponse[] = [
    {
      intent_id: "0x123...abc",
      source_chain: "ethereum",
      dest_chain: "starknet",
      source_token: "USDC",
      dest_token: "USDC",
      amount: "100000000",
      status: "completed",
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      source_complete_txid: "0xdef...456",
      dest_fill_txid: "0xabc...123",
      commitment: "0xabc",
      deadline: Date.now() + 86400000,
      has_privacy: true
    }
  ]

  let intents = rawIntents;
  if (statusFilter) {
    intents = intents.filter(i => i.status === statusFilter);
  }
  if (chainFilter) {
    intents = intents.filter(i => i.source_chain === chainFilter || i.dest_chain === chainFilter);
  }

  return {
    intents: intents.slice(0, limit),
    count: intents.length,
    isLoading: false,
    error: null,
    refetch: async () => {}
  }
}

export function formatChainName(chain: string) {
  if (chain === "ethereum") return "Ethereum Sepolia"
  if (chain === "starknet") return "Starknet Sepolia"
  return chain
}

export function formatTimeAgo(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} minutes ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} hours ago`;
  if (seconds < 604800) return `${Math.floor(seconds / 86400)} days ago`;

  return date.toLocaleDateString();
}

export function formatAmount(amount: string, decimals: number = 6): string {
  try {
    const value = BigInt(amount);
    const divisor = BigInt(10 ** decimals);
    const quotient = value / divisor;
    const remainder = value % divisor;

    if (remainder === BigInt(0)) {
      return quotient.toString();
    }

    const remainderStr = remainder.toString().padStart(decimals, "0");
    const trimmedRemainder = remainderStr.slice(0, Math.min(6, decimals)).replace(/0+$/, "");

    if (trimmedRemainder === "") {
      return quotient.toString();
    }

    return `${quotient}.${trimmedRemainder}`;
  } catch (error) {
    console.error("Error formatting amount:", error);
    return amount;
  }
}
