import { defineConfig } from "apibara/config";

export default defineConfig({
  runtimeConfig: {
    shadowswap: {
      // Starting block for mainnet - update this to the block where contract was deployed
      startingBlock: 6946374,
      streamUrl: "https://mainnet.starknet.a5a.ch",
      contractAddress:
        "0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114",
    },
  },
});
