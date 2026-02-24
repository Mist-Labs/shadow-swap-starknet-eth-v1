import { hash } from 'starknet';

// All event names from the Cairo contract (ShadowSettlement)
// Formula: sn_keccak(PascalCaseName) — confirmed against on-chain data
const events = [
  'CommitmentAdded',
  'BatchProcessed',
  'MerkleRootUpdated',
  'IntentMarkedSettled',
  'IntentSettled',
  'RemoteRootSynced',
  'RemoteRootVerified',
  'TokenWhitelistUpdated',
  'RelayerStatusChanged',
  'RootVerifierStatusChanged',
  'BatchConfigUpdated',
  'Paused',
  'Unpaused',
];

console.log('StarkNet Event Selectors:\n');

events.forEach(eventName => {
  const selector = hash.getSelectorFromName(eventName);
  console.log(`${eventName}:`);
  console.log(`  "${selector}",`);
  console.log('');
});
