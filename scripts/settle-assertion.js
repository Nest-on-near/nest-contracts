/**
 * Settle an undisputed assertion after liveness expires
 *
 * Usage: node settle-assertion.js '[assertion_id_array]'
 *
 * The assertion_id should be the JSON array format returned by the oracle,
 * e.g., '[1,2,3,...,32]'
 */

import { connect, keyStores, utils } from 'near-api-js';
import { readFileSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import 'dotenv/config';

const CONFIG = {
  networkId: process.env.NETWORK_ID || 'testnet',
  nodeUrl: 'https://rpc.testnet.near.org',
  oracleContract: process.env.ORACLE_CONTRACT || 'nest-oracle-3.testnet',
  accountId: process.env.NEAR_ACCOUNT_ID,
  privateKey: process.env.NEAR_PRIVATE_KEY,
};

async function getKeyStore() {
  const keyStore = new keyStores.InMemoryKeyStore();

  if (CONFIG.privateKey) {
    const keyPair = utils.KeyPair.fromString(CONFIG.privateKey);
    await keyStore.setKey(CONFIG.networkId, CONFIG.accountId, keyPair);
  } else {
    const credPath = join(homedir(), '.near-credentials', CONFIG.networkId, `${CONFIG.accountId}.json`);
    if (existsSync(credPath)) {
      const creds = JSON.parse(readFileSync(credPath, 'utf8'));
      const keyPair = utils.KeyPair.fromString(creds.private_key);
      await keyStore.setKey(CONFIG.networkId, CONFIG.accountId, keyPair);
    } else {
      throw new Error(`No credentials found.`);
    }
  }

  return keyStore;
}

async function main() {
  const assertionIdArg = process.argv[2];

  if (!assertionIdArg) {
    console.error('Usage: node settle-assertion.js \'[1,2,3,...,32]\'');
    process.exit(1);
  }

  let assertionId;
  try {
    assertionId = JSON.parse(assertionIdArg);
  } catch (e) {
    console.error('Invalid assertion_id format. Expected JSON array.');
    process.exit(1);
  }

  const keyStore = await getKeyStore();
  const near = await connect({
    networkId: CONFIG.networkId,
    keyStore,
    nodeUrl: CONFIG.nodeUrl,
  });

  const account = await near.account(CONFIG.accountId);
  console.log(`Connected as ${CONFIG.accountId}`);

  // Check assertion status first
  const assertion = await account.viewFunction({
    contractId: CONFIG.oracleContract,
    methodName: 'get_assertion',
    args: { assertion_id: assertionId },
  });

  if (!assertion) {
    console.error('Assertion not found');
    process.exit(1);
  }

  console.log('Assertion:', JSON.stringify(assertion, null, 2));

  if (assertion.settled) {
    console.log('Assertion is already settled.');
    process.exit(0);
  }

  if (assertion.disputer) {
    console.log('Assertion is disputed. Use resolve_disputed_assertion instead.');
    process.exit(1);
  }

  const now = Date.now() * 1_000_000; // Convert to nanoseconds
  const expirationNs = BigInt(assertion.expiration_time_ns);

  if (BigInt(now) < expirationNs) {
    const remainingMs = Number((expirationNs - BigInt(now)) / 1_000_000n);
    const remainingSec = Math.ceil(remainingMs / 1000);
    console.log(`Liveness period not expired. Wait ${remainingSec} more seconds.`);
    process.exit(1);
  }

  console.log('Settling assertion...');

  const result = await account.functionCall({
    contractId: CONFIG.oracleContract,
    methodName: 'settle_assertion',
    args: { assertion_id: assertionId },
    gas: '100000000000000',
  });

  console.log('✓ Assertion settled!');
  console.log('Transaction:', result.transaction.hash);

  // Log events
  for (const outcome of result.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      console.log('Log:', log);
    }
  }
}

main().catch(console.error);
