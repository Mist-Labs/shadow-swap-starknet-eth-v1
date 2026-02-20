

export function useTokenUSDValue(tokenSymbol: string, amount: string) {
  // Mock prices
  const prices: Record<string, number> = {
    ETH: 2500,
    USDC: 1,
    USDT: 1,
    MNT: 0.8
  }

  const price = prices[tokenSymbol] || 0
  const usdValue = amount && !isNaN(parseFloat(amount)) ? parseFloat(amount) * price : null
  
  return {
    usdValue,
    isLoading: false,
    price
  }
}
