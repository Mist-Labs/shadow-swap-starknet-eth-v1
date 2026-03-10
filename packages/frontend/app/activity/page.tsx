"use client"
"use no memo"

import { useState, useMemo } from "react"

import Header from "@/components/shared/header"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import {
  useReactTable,
  getCoreRowModel,
  getPaginationRowModel,
  flexRender,
  type ColumnDef,
} from "@tanstack/react-table"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import {
  Search,
  ExternalLink,
  Copy,
  Check,
  ArrowRight,
  RefreshCw,
  AlertCircle,
  Wifi,
  Loader2,
} from "lucide-react"
import { useBridgeIntents, formatChainName } from "@/hooks/useBridgeIntents"
import { useAccount } from "wagmi"
import { useAccount as useStarknetAccount } from "@starknet-react/core"
import { deriveViewKey } from "@/lib/crypto"
import type { IntentStatusResponse, IntentStatus } from "@/lib/api"
import type { ChainType } from "@/lib/tokens"
import { formatTimeAgo, formatAmount, getTokenSymbol } from "@/lib/utils"
import { BridgeStatusBadge, BridgeStatusIcon } from "@/components/bridge/BridgeStatus"
export default function ActivityPage() {
  const { address: evmAddress, isConnected: isEvmConnected } = useAccount()
  const { address: starknetAddress, isConnected: isStarknetConnected } = useStarknetAccount()

  const [searchQuery, setSearchQuery] = useState("")
  const [statusFilter, setStatusFilter] = useState<string>("all")
  const [networkFilter, setNetworkFilter] = useState<string>("all")
  const [selectedTx, setSelectedTx] = useState<IntentStatusResponse | null>(null)
  const [copiedField, setCopiedField] = useState<string | null>(null)

  // Automatically derive view_key(s) if wallets are connected
  const derivedViewKey = useMemo(() => {
    const keys: string[] = []

    if (isEvmConnected && evmAddress) {
      try {
        // We derive both potential view keys (for EVM-bound and Starknet-bound txs)
        // because the derivation differs (31 vs 32 bytes) for EVM sources.
        keys.push(deriveViewKey(evmAddress, "ethereum", "ethereum"))
        keys.push(deriveViewKey(evmAddress, "ethereum", "starknet"))
      } catch (e) {
        console.warn("Failed to derive EVM view key", e)
      }
    }

    if (isStarknetConnected && starknetAddress) {
      try {
        // Starknet source keys are always 31-byte poseidon hashes currently
        keys.push(deriveViewKey(starknetAddress as string, "starknet", "ethereum"))
      } catch (e) {
        console.warn("Failed to derive Starknet view key", e)
      }
    }

    return keys.length > 0 ? keys.join(",") : undefined
  }, [evmAddress, starknetAddress, isEvmConnected, isStarknetConnected])

  // Fetch bridge intents — when derivedViewKey is present, it securely filters the backend
  const filterStatus = statusFilter !== "all" ? (statusFilter as IntentStatus) : undefined
  const filterChain = networkFilter !== "all" ? (networkFilter as ChainType) : undefined

  const { intents, isLoading, error, refetch } = useBridgeIntents({
    status: filterStatus,
    chain: filterChain,
    limit: 50,
    viewKey: derivedViewKey,
  })

  // Client-side filtering for search queries
  const filteredTransactions = useMemo(() => {
    return intents.filter((intent) => {
      const matchesSearch =
        searchQuery === "" ||
        (intent.intent_id && intent.intent_id.toLowerCase().includes(searchQuery.toLowerCase())) ||
        intent.source_chain.toLowerCase().includes(searchQuery.toLowerCase()) ||
        intent.dest_chain.toLowerCase().includes(searchQuery.toLowerCase()) ||
        intent.source_token.toLowerCase().includes(searchQuery.toLowerCase()) ||
        (intent.commitment && intent.commitment.toLowerCase().includes(searchQuery.toLowerCase()))

      return matchesSearch
    })
  }, [intents, searchQuery])

  // Table columns configuration
  const columns = useMemo<ColumnDef<IntentStatusResponse>[]>(() => [
    {
      accessorKey: "status",
      header: "Status",
      cell: ({ row }) => (
        <div className="flex items-center gap-2">
          <BridgeStatusIcon status={row.original.status} />
          <BridgeStatusBadge status={row.original.status} />
        </div>
      ),
    },
    {
      accessorKey: "created_at",
      header: "Time",
      cell: ({ row }) => <span className="text-neutral-300">{formatTimeAgo(row.original.created_at)}</span>,
    },
    {
      id: "route",
      header: "Route",
      cell: ({ row }) => (
        <div className="flex items-center gap-2">
          <span className="text-neutral-300">{formatChainName(row.original.source_chain)}</span>
          <ArrowRight className="h-3 w-3 text-orange-500" />
          <span className="text-neutral-300">{formatChainName(row.original.dest_chain)}</span>
        </div>
      ),
    },
    {
      accessorKey: "amount",
      header: "Amount",
      cell: ({ row }) => (
        <div>
          <div className="font-medium text-white">
            {formatAmount(row.original.amount)} {getTokenSymbol(row.original.source_token)}
          </div>
          {row.original.has_privacy && (
            <div className="text-xs text-purple-400">🔒 Private</div>
          )}
        </div>
      ),
    },
    {
      id: "fee",
      header: "Fee",
      cell: () => <span className="text-neutral-300">~0.2%</span>,
    },
    {
      accessorKey: "intent_id",
      header: "Intent ID",
      cell: ({ row }) => (
        <div className="flex items-center gap-2">
          <span className="font-mono text-sm text-neutral-300">
            {row.original.intent_id ? `${row.original.intent_id.slice(0, 6)}...${row.original.intent_id.slice(-4)}` : "N/A"}
          </span>
          {row.original.intent_id && (
            <button
              onClick={(e) => {
                e.stopPropagation()
                copyToClipboard(row.original.intent_id, row.original.intent_id)
              }}
              className="text-neutral-500 transition-colors hover:text-orange-500"
            >
              {copiedField === row.original.intent_id ? (
                <Check className="h-4 w-4 text-green-500" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </button>
          )}
        </div>
      ),
    },
    {
      id: "actions",
      header: () => <div className="text-right">Actions</div>,
      cell: ({ row }) => (
        <div className="text-right">
          <Button
            size="sm"
            variant="outline"
            className="border-neutral-700 bg-neutral-800 hover:border-orange-500/50 hover:bg-neutral-700"
            onClick={(e) => {
              e.stopPropagation()
              setSelectedTx(row.original)
            }}
          >
            View Details
          </Button>
        </div>
      ),
    },
  ], [copiedField])

  // eslint-disable-next-line
  const table = useReactTable({
    data: filteredTransactions,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: {
      pagination: {
        pageSize: 10,
      },
    },
  })



  const copyToClipboard = (text: string, field: string) => {
    navigator.clipboard.writeText(text)
    setCopiedField(field)
    setTimeout(() => setCopiedField(null), 2000)
  }

  // Helper to extract token symbol from address or return as-is if already a symbol




  return (
    <div className="min-h-screen bg-black">
      <Header />

      {/* Main Content */}
      <main className="px-4 pb-12 pt-24 sm:px-6">
        <div className="mx-auto max-w-7xl">
          {/* Header */}
          <div className="mb-8">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <h1 className="mb-2 text-3xl font-bold text-white sm:text-4xl">Transaction Activity</h1>
                <div className="flex items-center gap-3">
                  <p className="text-neutral-400">View and track your cross-chain bridge transactions</p>
                  <div className="flex items-center gap-1.5 text-xs text-green-500">
                    <Wifi className="h-3 w-3" />
                    <span>Live</span>
                  </div>
                </div>
              </div>
              <Button
                onClick={() => refetch()}
                disabled={isLoading}
                variant="outline"
                className="border-neutral-700 bg-neutral-800 hover:bg-neutral-700"
              >
                <RefreshCw className={`mr-2 h-4 w-4 ${isLoading ? "animate-spin" : ""}`} />
                Refresh
              </Button>
            </div>

            {/* View Key Banner Removed per User Request */}
          </div>

          {/* Error State */}
          {error && (
            <div className="mb-6 flex items-center gap-3 rounded-lg border border-red-500/20 bg-red-500/10 p-4 text-red-500">
              <AlertCircle className="h-5 w-5" />
              <div>
                <p className="font-medium">Error loading transactions</p>
                <p className="text-sm text-red-400">
                  {error instanceof Error ? error.message : "Unable to reach the backend. Please try again."}
                </p>
              </div>
            </div>
          )}

          {/* Filters */}
          <div className="mb-6 rounded-lg border border-neutral-800 bg-neutral-900 p-6">
            <div className="grid grid-cols-1 gap-4 md:grid-cols-4">
              {/* Search */}
              <div className="md:col-span-2">
                <div className="relative">
                  <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 transform text-neutral-500" />
                  <Input
                    placeholder="Search by intent ID, chain, token, commitment..."
                    className="border-neutral-700 bg-neutral-800 pl-10 text-white"
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    disabled={isLoading}
                  />
                </div>
              </div>

              {/* Status Filter */}
              <div>
                <Select value={statusFilter} onValueChange={setStatusFilter} disabled={isLoading}>
                  <SelectTrigger className="border-neutral-700 bg-neutral-800 text-white">
                    <SelectValue placeholder="All Statuses" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All Statuses</SelectItem>
                    <SelectItem value="created">Created</SelectItem>
                    <SelectItem value="committed">Committed</SelectItem>
                    <SelectItem value="filled">Filled</SelectItem>
                    <SelectItem value="completed">Completed</SelectItem>
                    <SelectItem value="refunded">Refunded</SelectItem>
                    <SelectItem value="failed">Failed</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {/* Network Filter */}
              <div>
                <Select value={networkFilter} onValueChange={setNetworkFilter} disabled={isLoading}>
                  <SelectTrigger className="border-neutral-700 bg-neutral-800 text-white">
                    <SelectValue placeholder="All Networks" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All Networks</SelectItem>
                    <SelectItem value="starknet">Starknet</SelectItem>
                    <SelectItem value="ethereum">Ethereum</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
          </div>

          <div className="overflow-hidden rounded-lg border border-neutral-800 bg-neutral-900">
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id} className="border-neutral-800 hover:bg-neutral-800/50">
                    {headerGroup.headers.map((header) => (
                      <TableHead key={header.id} className="text-neutral-400">
                        {header.isPlaceholder
                          ? null
                          : flexRender(header.column.columnDef.header, header.getContext())}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow>
                    <TableCell colSpan={columns.length} className="py-12 text-center">
                      <div className="flex items-center justify-center gap-2 text-neutral-500">
                        <Loader2 className="h-5 w-5 animate-spin" />
                        <span>Loading transactions...</span>
                      </div>
                    </TableCell>
                  </TableRow>
                ) : table.getRowModel().rows.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={columns.length} className="py-16 text-center">
                      <div className="flex flex-col items-center gap-3 text-neutral-500">
                        <div className="rounded-full border border-neutral-700 bg-neutral-800 p-4">
                          <svg className="h-8 w-8 text-neutral-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                          </svg>
                        </div>
                        <div>
                          <p className="font-medium text-neutral-400">
                            {searchQuery || statusFilter !== "all" || networkFilter !== "all"
                              ? "No transactions match your filters"
                              : "No transactions yet"}
                          </p>
                          <p className="mt-1 text-sm">
                            {searchQuery || statusFilter !== "all" || networkFilter !== "all"
                              ? "Try adjusting your search or filters"
                              : "Bridge your first transaction to see activity here"}
                          </p>
                        </div>
                      </div>
                    </TableCell>
                  </TableRow>
                ) : (
                  table.getRowModel().rows.map((row) => (
                    <TableRow
                      key={row.id}
                      data-state={row.getIsSelected() && "selected"}
                      className="cursor-pointer border-neutral-800 hover:bg-neutral-800/30"
                      onClick={() => setSelectedTx(row.original)}
                    >
                      {row.getVisibleCells().map((cell) => (
                        <TableCell key={cell.id}>
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>

          {/* Pagination */}
          <div className="mt-6 flex items-center justify-between">
            <p className="text-sm text-neutral-500">
              Showing {table.getState().pagination.pageIndex * table.getState().pagination.pageSize + 1}-
              {Math.min(
                (table.getState().pagination.pageIndex + 1) * table.getState().pagination.pageSize,
                table.getFilteredRowModel().rows.length
              )}{" "}
              of {table.getFilteredRowModel().rows.length} transactions
            </p>
            <div className="flex gap-2">
              <Button
                variant="outline"
                className="border-neutral-700 bg-neutral-900 hover:bg-neutral-800"
                onClick={() => table.previousPage()}
                disabled={!table.getCanPreviousPage()}
              >
                Previous
              </Button>
              <Button
                variant="outline"
                className="border-neutral-700 bg-neutral-900 hover:bg-neutral-800"
                onClick={() => table.nextPage()}
                disabled={!table.getCanNextPage()}
              >
                Next
              </Button>
            </div>
          </div>
        </div>
      </main>

      {/* Transaction Detail Modal */}
      <Dialog open={!!selectedTx} onOpenChange={() => setSelectedTx(null)}>
        <DialogContent className="max-w-2xl border border-neutral-800 bg-neutral-900 text-white">
          <DialogHeader>
            <DialogTitle className="text-2xl font-bold">Transaction Details</DialogTitle>
            <DialogDescription className="text-neutral-400">
              View complete information about this bridge transaction
            </DialogDescription>
          </DialogHeader>

          {selectedTx && (
            <div className="space-y-6">
              {/* Status */}
              <div className="flex items-center justify-between">
                <span className="text-neutral-400">Status</span>
                <div className="flex items-center gap-2">
                  <BridgeStatusIcon status={selectedTx.status} />
                  <BridgeStatusBadge status={selectedTx.status} />
                </div>
              </div>

              {/* Route */}
              <div className="flex items-center justify-between">
                <span className="text-neutral-400">Route</span>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="border-neutral-700">
                    {formatChainName(selectedTx.source_chain)}
                  </Badge>
                  <ArrowRight className="h-4 w-4 text-orange-500" />
                  <Badge variant="outline" className="border-neutral-700">
                    {formatChainName(selectedTx.dest_chain)}
                  </Badge>
                </div>
              </div>

              {/* Amount */}
              <div className="flex items-center justify-between">
                <span className="text-neutral-400">Amount</span>
                <div className="text-right">
                  <div className="font-medium text-white">
                    {formatAmount(selectedTx.amount, 6)} {getTokenSymbol(selectedTx.source_token)}
                  </div>
                  <div className="text-sm text-neutral-500">→ {getTokenSymbol(selectedTx.dest_token)}</div>
                </div>
              </div>

              {/* Privacy */}
              {selectedTx.has_privacy && (
                <div className="flex items-center justify-between">
                  <span className="text-neutral-400">Privacy</span>
                  <Badge variant="outline" className="border-purple-500/20 bg-purple-500/10 text-purple-400">
                    🔒 Privacy-Enhanced
                  </Badge>
                </div>
              )}

              {/* Deadline — not provided by backend; show updated_at instead */}
              <div className="flex items-center justify-between">
                <span className="text-neutral-400">Last Updated</span>
                <span className="text-white">
                  {new Date(Number(selectedTx.updated_at) * 1000).toLocaleString()}
                </span>
              </div>

              {/* Time */}
              <div className="flex items-center justify-between">
                <span className="text-neutral-400">Created</span>
                <span className="text-white">
                  {new Date(Number(selectedTx.created_at) * 1000).toLocaleString()}
                </span>
              </div>

              {/* Intent ID */}
              <div>
                <div className="mb-2 text-neutral-400">Intent ID</div>
                <div className="flex items-center gap-2 rounded border border-neutral-700 bg-neutral-800 p-3">
                  <span className="flex-1 truncate font-mono text-sm text-white">{selectedTx.intent_id}</span>
                  <button
                    onClick={() => copyToClipboard(selectedTx.intent_id, "modal-intent")}
                    className="text-neutral-500 transition-colors hover:text-orange-500"
                  >
                    {copiedField === "modal-intent" ? (
                      <Check className="h-4 w-4 text-green-500" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </button>
                </div>
              </div>

              {/* Commitment */}
              <div>
                <div className="mb-2 text-neutral-400">Commitment</div>
                <div className="flex items-center gap-2 rounded border border-neutral-700 bg-neutral-800 p-3">
                  <span className="flex-1 truncate font-mono text-sm text-white">{selectedTx.commitment}</span>
                  <button
                    onClick={() => copyToClipboard(selectedTx.commitment, "modal-commitment")}
                    className="text-neutral-500 transition-colors hover:text-orange-500"
                  >
                    {copiedField === "modal-commitment" ? (
                      <Check className="h-4 w-4 text-green-500" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </button>
                </div>
              </div>

              {/* Transaction Hashes */}
              {selectedTx.dest_tx_hash && (
                <div>
                  <div className="mb-2 text-neutral-400">Destination Transaction</div>
                  <div className="flex items-center gap-2 rounded border border-neutral-700 bg-neutral-800 p-3">
                    <span className="flex-1 truncate font-mono text-sm text-white">{selectedTx.dest_tx_hash}</span>
                    <button
                      onClick={() => copyToClipboard(selectedTx.dest_tx_hash!, "modal-fill")}
                      className="text-neutral-500 transition-colors hover:text-orange-500"
                    >
                      {copiedField === "modal-fill" ? (
                        <Check className="h-4 w-4 text-green-500" />
                      ) : (
                        <Copy className="h-4 w-4" />
                      )}
                    </button>
                    <a
                      href={`${selectedTx.dest_chain === "ethereum"
                        ? process.env.NEXT_PUBLIC_ETHEREUM_EXPLORER
                        : process.env.NEXT_PUBLIC_STARKNET_EXPLORER
                        }/tx/${selectedTx.dest_tx_hash}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-neutral-500 transition-colors hover:text-orange-500"
                    >
                      <ExternalLink className="h-4 w-4" />
                    </a>
                  </div>
                </div>
              )}

              {selectedTx.settle_tx_hash && (
                <div>
                  <div className="mb-2 text-neutral-400">Settlement Transaction</div>
                  <div className="flex items-center gap-2 rounded border border-neutral-700 bg-neutral-800 p-3">
                    <span className="flex-1 truncate font-mono text-sm text-white">
                      {selectedTx.settle_tx_hash}
                    </span>
                    <button
                      onClick={() => copyToClipboard(selectedTx.settle_tx_hash!, "modal-settle")}
                      className="text-neutral-500 transition-colors hover:text-orange-500"
                    >
                      {copiedField === "modal-settle" ? (
                        <Check className="h-4 w-4 text-green-500" />
                      ) : (
                        <Copy className="h-4 w-4" />
                      )}
                    </button>
                    <a
                      href={`${selectedTx.source_chain === "ethereum"
                        ? process.env.NEXT_PUBLIC_ETHEREUM_EXPLORER
                        : process.env.NEXT_PUBLIC_STARKNET_EXPLORER
                        }/tx/${selectedTx.settle_tx_hash}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-neutral-500 transition-colors hover:text-orange-500"
                    >
                      <ExternalLink className="h-4 w-4" />
                    </a>
                  </div>
                </div>
              )}

              {selectedTx.source_settle_tx_hash && (
                <div>
                  <div className="mb-2 text-neutral-400">Source Settlement Transaction</div>
                  <div className="flex items-center gap-2 rounded border border-neutral-700 bg-neutral-800 p-3">
                    <span className="flex-1 truncate font-mono text-sm text-white">
                      {selectedTx.source_settle_tx_hash}
                    </span>
                    <button
                      onClick={() => copyToClipboard(selectedTx.source_settle_tx_hash!, "modal-source-settle")}
                      className="text-neutral-500 transition-colors hover:text-orange-500"
                    >
                      {copiedField === "modal-source-settle" ? (
                        <Check className="h-4 w-4 text-green-500" />
                      ) : (
                        <Copy className="h-4 w-4" />
                      )}
                    </button>
                    <a
                      href={`${selectedTx.source_chain === "ethereum"
                        ? process.env.NEXT_PUBLIC_ETHEREUM_EXPLORER
                        : process.env.NEXT_PUBLIC_STARKNET_EXPLORER
                        }/tx/${selectedTx.source_settle_tx_hash}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-neutral-500 transition-colors hover:text-orange-500"
                    >
                      <ExternalLink className="h-4 w-4" />
                    </a>
                  </div>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
