// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {ShadowSettlement} from "../src/ShadowSettlement.sol";

/**
 * @title DeploySepoliaContracts
 * @notice Deploy ShadowSettlement on Ethereum Sepolia
 * @dev Usage: forge script script/Deployer.s.sol:DeploySepoliaContracts --rpc-url $SEPOLIA_RPC_URL --broadcast --verify
 */
contract DeploySepoliaContracts is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address owner = vm.envAddress("OWNER_ADDRESS");
        address relayer = vm.envAddress("RELAYER_ADDRESS");

        console.log("=== Sepolia (EVM) Deployment ===");
        console.log("Deployer:", vm.addr(deployerPrivateKey));
        console.log("Owner:", owner);
        console.log("Relayer:", relayer);
        console.log("Chain ID:", block.chainid);
        console.log("");

        vm.startBroadcast(deployerPrivateKey);

        console.log("Deploying ShadowSettlement...");
        ShadowSettlement settlement = new ShadowSettlement(owner, relayer);
        console.log("ShadowSettlement deployed at:", address(settlement));
        console.log("");

        vm.stopBroadcast();

        console.log("=== SEPOLIA DEPLOYMENT COMPLETE ===");
        console.log("ShadowSettlement:", address(settlement));
        console.log("");
        console.log("Add to .env:");
        console.log("EVM_INTENT_POOL_ADDRESS=", address(settlement));
        console.log("EVM_SETTLEMENT_ADDRESS=", address(settlement));
        console.log("");
        console.log("Next steps:");
        console.log("1. Whitelist tokens via setTokenWhitelist()");
        console.log("2. Fund contract with tokens for settlement payouts");
        console.log(
            "3. Configure batch settings if needed (default: size=10, timeout=30s)"
        );
    }
}

/**
 * @title ConfigureTokens
 * @notice Configure supported tokens on ShadowSettlement
 * @dev Usage: forge script script/Deployer.s.sol:ConfigureTokens --rpc-url $SEPOLIA_RPC_URL --broadcast
 */
contract ConfigureTokens is Script {
    // Common token addresses
    address constant NATIVE_ETH =
        address(0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE);

    // Sepolia testnet token addresses
    address constant USDC = 0x28650373758d75a8fF0B22587F111e47BAC34e21; // Example - update with actual
    address constant USDT = 0x89F4f0e13997Ca27cEB963DEE291C607e4E59923; // Example - update with actual

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address payable settlement = payable(
            vm.envAddress("EVM_SETTLEMENT_ADDRESS")
        ); // FIX: address payable

        console.log("=== Configuring Sepolia Tokens ===");
        console.log("Settlement:", settlement);
        console.log("Chain ID:", block.chainid);
        console.log("");

        vm.startBroadcast(deployerPrivateKey);

        console.log("Whitelisting Native ETH and tokens...");
        ShadowSettlement(settlement).setTokenWhitelist(NATIVE_ETH, true);
        ShadowSettlement(settlement).setTokenWhitelist(USDC, true);
        ShadowSettlement(settlement).setTokenWhitelist(USDT, true);
        console.log("Tokens whitelisted!");

        vm.stopBroadcast();

        console.log("");
        console.log("=== TOKEN CONFIGURATION COMPLETE ===");
        console.log("Whitelisted tokens:");
        console.log("  - Native ETH, USDT, USDC");
    }
}

/**
 * @title FundSettlement
 * @notice Fund the settlement contract with tokens for payouts
 * @dev Usage: forge script script/Deployer.s.sol:FundSettlement --rpc-url $SEPOLIA_RPC_URL --broadcast
 */
contract FundSettlement is Script {
    address constant NATIVE_ETH =
        address(0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE);

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address settlement = vm.envAddress("EVM_SETTLEMENT_ADDRESS");

        // Funding amounts (adjust as needed)
        uint256 ethAmount = 0.1 ether; // Fund with 0.1 ETH for testing

        console.log("=== Funding Settlement Contract ===");
        console.log("Settlement:", settlement);
        console.log("Funder:", vm.addr(deployerPrivateKey));
        console.log("");

        vm.startBroadcast(deployerPrivateKey);

        console.log("Sending", ethAmount, "ETH...");
        // Note: Since contract rejects direct ETH, we need to use an ERC20 wrapper
        // or fund via NEAR bridge transfers in production
        console.log("WARNING: Contract rejects direct ETH transfers");
        console.log("Tokens must arrive via NEAR bridge ERC20 transfers");

        // For ERC20 tokens, use this pattern:
        /*
        IERC20 usdc = IERC20(SEPOLIA_USDC);
        uint256 usdcAmount = 1000 * 10**6; // 1000 USDC
        usdc.transfer(settlement, usdcAmount);
        console.log("Transferred", usdcAmount, "USDC");
        */

        vm.stopBroadcast();

        console.log("");
        console.log("Note: In production, settlement contract receives tokens");
        console.log(
            "automatically from NEAR 1Click bridge via standard ERC20 transfers"
        );
    }
}

/**
 * @title ConfigureBatch
 * @notice Update batch configuration (size and timeout)
 * @dev Usage: forge script script/Deployer.s.sol:ConfigureBatch --rpc-url $SEPOLIA_RPC_URL --broadcast
 */
contract ConfigureBatch is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address payable settlement = payable(
            vm.envAddress("EVM_SETTLEMENT_ADDRESS")
        ); // FIX: address payable

        // Batch configuration
        uint256 newBatchSize = 10; // Process when 10 commitments accumulated
        uint256 newTimeout = 30; // Or process after 30 seconds

        console.log("=== Configuring Batch Settings ===");
        console.log("Settlement:", settlement);
        console.log("New batch size:", newBatchSize);
        console.log("New timeout (seconds):", newTimeout);
        console.log("");

        vm.startBroadcast(deployerPrivateKey);

        ShadowSettlement(settlement).updateBatchConfig(
            newBatchSize,
            newTimeout
        );
        console.log("Batch configuration updated!");

        vm.stopBroadcast();

        console.log("");
        console.log("=== BATCH CONFIGURATION COMPLETE ===");
        console.log("Batch will process when:");
        console.log("  - ", newBatchSize, "commitments accumulated, OR");
        console.log(
            "  - ",
            newTimeout,
            "seconds elapsed since first commitment"
        );
    }
}

/**
 * @title VerifyDeployment
 * @notice Verify deployment and check contract state
 * @dev Usage: forge script script/Deployer.s.sol:VerifyDeployment --rpc-url $SEPOLIA_RPC_URL
 */
contract VerifyDeployment is Script {
    function run() external view {
        address payable settlement = payable(
            vm.envAddress("EVM_SETTLEMENT_ADDRESS")
        );
        address relayer = vm.envAddress("RELAYER_ADDRESS");

        console.log("=== Verifying Deployment ===");
        console.log("Settlement:", settlement);
        console.log("");

        ShadowSettlement s = ShadowSettlement(settlement);

        console.log("Owner:", s.owner());
        console.log("Relayer authorized:", s.isRelayerAuthorized(relayer));
        console.log("Root verifier authorized:", s.isRootVerifier(relayer));
        console.log("");

        console.log("Merkle root:", vm.toString(s.getMerkleRoot()));
        console.log("Next leaf index:", s.nextLeafIndex());
        console.log("");

        (uint256 count, uint64 firstTime, uint256 remaining) = s
            .getPendingBatchInfo();
        console.log("Pending batch count:", count);
        console.log("Batch first submission time:", firstTime);
        console.log("Time remaining:", remaining);
        console.log("");

        console.log("Batch size:", s.batchSize());
        console.log("Batch timeout:", s.batchTimeout());
        console.log("");

        console.log("Paused:", s.paused());
        console.log("");

        console.log("=== DEPLOYMENT VERIFIED ===");
    }
}
