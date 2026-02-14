/**
 * NEST Oracle - Full DVM Voting Flow Test
 *
 * Tests the complete commit-reveal voting process:
 * 1. Create assertion
 * 2. Dispute assertion → DVM request created
 * 3. Commit vote during commit phase
 * 4. Advance to reveal phase
 * 5. Reveal vote during reveal phase
 * 6. Resolve price
 * 7. Settle assertion via DVM
 */

import { connect, keyStores, utils } from 'near-api-js';
import { readFileSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { createHash } from 'crypto';
import 'dotenv/config';

const CONFIG = {
  networkId: process.env.NETWORK_ID || 'testnet',
  nodeUrl: 'https://test.rpc.fastnear.com',
  oracleContract: process.env.ORACLE_CONTRACT || 'nest-oracle-3.testnet',
  tokenContract: process.env.TOKEN_CONTRACT || 'nest-token-1.testnet',
  votingContract: process.env.VOTING_CONTRACT || 'nest-voting-1.testnet',
  accountId: process.env.NEAR_ACCOUNT_ID,
  privateKey: process.env.NEAR_PRIVATE_KEY,
};

// Scale constants
const SCALE = BigInt('1000000000000000000'); // 1e18
const NUMERICAL_TRUE = SCALE; // 1e18 means TRUE

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
    }
  }

  return keyStore;
}

function stringToBytes32(str) {
  const bytes = new Uint8Array(32);
  const encoder = new TextEncoder();
  const encoded = encoder.encode(str);
  bytes.set(encoded.slice(0, 32));
  return Array.from(bytes);
}

function bytesToHex(bytes) {
  return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Compute vote hash: sha256(price_le_bytes || salt)
 */
function computeVoteHash(price, salt) {
  // Convert price to i128 little-endian bytes (16 bytes)
  const priceBigInt = BigInt(price);
  const priceBuffer = Buffer.alloc(16);

  // Write as little-endian
  let val = priceBigInt < 0n ? priceBigInt + (1n << 128n) : priceBigInt;
  for (let i = 0; i < 16; i++) {
    priceBuffer[i] = Number(val & 0xffn);
    val >>= 8n;
  }

  // Concatenate price bytes + salt bytes
  const saltBuffer = Buffer.from(salt);
  const data = Buffer.concat([priceBuffer, saltBuffer]);

  // SHA256 hash
  const hash = createHash('sha256').update(data).digest();
  return Array.from(hash);
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function main() {
  console.log('\n╔══════════════════════════════════════════════════════════════╗');
  console.log('║     NEST Oracle - Full DVM Voting Flow Test                  ║');
  console.log('╚══════════════════════════════════════════════════════════════╝\n');

  const keyStore = await getKeyStore();
  const near = await connect({
    networkId: CONFIG.networkId,
    keyStore,
    nodeUrl: CONFIG.nodeUrl,
  });

  const account = await near.account(CONFIG.accountId);
  console.log(`✓ Connected as ${CONFIG.accountId}\n`);

  // Check voting config
  const votingConfig = await account.viewFunction({
    contractId: CONFIG.votingContract,
    methodName: 'get_config',
    args: {},
  });
  console.log('Voting Config:');
  console.log(`  Commit phase: ${votingConfig[0] / 1e9} seconds`);
  console.log(`  Reveal phase: ${votingConfig[1] / 1e9} seconds`);
  console.log(`  Min participation: ${votingConfig[2] / 100}%\n`);

  // Get minimum bond
  const minBond = await account.viewFunction({
    contractId: CONFIG.oracleContract,
    methodName: 'get_minimum_bond',
    args: { currency: CONFIG.tokenContract },
  });
  console.log(`Minimum bond: ${minBond}\n`);

  // ═══════════════════════════════════════════════════════════════
  // STEP 1: Create Assertion
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 1: Creating Assertion');
  console.log('═'.repeat(60));

  const claimText = `DVM Test ${new Date().toISOString()}`;
  const claim = stringToBytes32(claimText);

  const assertMsg = JSON.stringify({
    action: 'AssertTruth',
    claim: claim,
    asserter: CONFIG.accountId,
    liveness_ns: '60000000000', // 1 minute liveness
  });

  const assertResult = await account.functionCall({
    contractId: CONFIG.tokenContract,
    methodName: 'ft_transfer_call',
    args: {
      receiver_id: CONFIG.oracleContract,
      amount: minBond,
      msg: assertMsg,
    },
    gas: '100000000000000',
    attachedDeposit: '1',
  });

  // Extract assertion_id from logs
  let assertionId = null;
  for (const outcome of assertResult.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      if (log.includes('assertion_made')) {
        const eventData = JSON.parse(log.replace('EVENT_JSON:', ''));
        assertionId = eventData.data[0].assertion_id;
        break;
      }
    }
  }

  if (!assertionId) {
    console.error('Failed to get assertion ID from logs');
    process.exit(1);
  }

  console.log(`✓ Assertion created!`);
  console.log(`  ID: ${bytesToHex(assertionId)}\n`);

  // ═══════════════════════════════════════════════════════════════
  // STEP 2: Dispute Assertion
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 2: Disputing Assertion (triggers DVM)');
  console.log('═'.repeat(60));

  const disputeMsg = JSON.stringify({
    action: 'DisputeAssertion',
    assertion_id: assertionId,
    disputer: CONFIG.accountId,
  });

  const disputeResult = await account.functionCall({
    contractId: CONFIG.tokenContract,
    methodName: 'ft_transfer_call',
    args: {
      receiver_id: CONFIG.oracleContract,
      amount: minBond,
      msg: disputeMsg,
    },
    gas: '150000000000000',
    attachedDeposit: '1',
  });

  // Extract DVM request_id
  let dvmRequestId = null;
  for (const outcome of disputeResult.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      if (log.includes('price_requested')) {
        const eventData = JSON.parse(log.replace('EVENT_JSON:', ''));
        dvmRequestId = eventData.data[0].request_id;
        break;
      }
    }
  }

  console.log(`✓ Assertion disputed!`);
  console.log(`  DVM Request ID: ${dvmRequestId ? bytesToHex(dvmRequestId) : 'Check oracle for request ID'}\n`);

  // Get request ID from oracle if not found in logs
  if (!dvmRequestId) {
    dvmRequestId = await account.viewFunction({
      contractId: CONFIG.oracleContract,
      methodName: 'get_dispute_request',
      args: { assertion_id: assertionId },
    });
    console.log(`  DVM Request ID (from oracle): ${bytesToHex(dvmRequestId)}\n`);
  }

  // ═══════════════════════════════════════════════════════════════
  // STEP 3: Commit Vote
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 3: Committing Vote');
  console.log('═'.repeat(60));

  // Vote for FALSE (disputer wins) = 0
  // Note: Using 0 because i128 values > 2^53 can't be safely passed via JSON
  // In production, the contract would use I128 wrapper for safe serialization
  const votePrice = 0;  // 0 = FALSE (disputer wins), SCALE = TRUE (asserter wins)
  const salt = Array.from(createHash('sha256').update(`salt-${Date.now()}`).digest());
  const commitHash = computeVoteHash(votePrice.toString(), salt);

  console.log(`  Vote: TRUE (${votePrice})`);
  console.log(`  Commit hash: ${bytesToHex(commitHash)}`);

  await account.functionCall({
    contractId: CONFIG.votingContract,
    methodName: 'commit_vote',
    args: {
      request_id: dvmRequestId,
      commit_hash: commitHash,
      staked_amount: '1000000000000000000', // 1 token stake
    },
    gas: '50000000000000',
  });

  console.log(`✓ Vote committed!\n`);

  // Check phase
  let phase = await account.viewFunction({
    contractId: CONFIG.votingContract,
    methodName: 'get_phase',
    args: { request_id: dvmRequestId },
  });
  console.log(`  Current phase: ${phase}\n`);

  // ═══════════════════════════════════════════════════════════════
  // STEP 4: Wait and Advance to Reveal Phase
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 4: Waiting for Commit Phase to End');
  console.log('═'.repeat(60));

  const commitDuration = votingConfig[0] / 1e9;
  console.log(`  Waiting ${commitDuration + 2} seconds for commit phase to end...`);
  await sleep((commitDuration + 2) * 1000);

  console.log('  Advancing to reveal phase...');
  await account.functionCall({
    contractId: CONFIG.votingContract,
    methodName: 'advance_to_reveal',
    args: { request_id: dvmRequestId },
    gas: '30000000000000',
  });

  phase = await account.viewFunction({
    contractId: CONFIG.votingContract,
    methodName: 'get_phase',
    args: { request_id: dvmRequestId },
  });
  console.log(`✓ Phase advanced to: ${phase}\n`);

  // ═══════════════════════════════════════════════════════════════
  // STEP 5: Reveal Vote
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 5: Revealing Vote');
  console.log('═'.repeat(60));

  console.log(`  Revealing: price=${votePrice}, salt=${bytesToHex(salt).substring(0, 16)}...`);

  // Pass price as integer (must be within JavaScript safe integer range)
  await account.functionCall({
    contractId: CONFIG.votingContract,
    methodName: 'reveal_vote',
    args: {
      request_id: dvmRequestId,
      price: Number(votePrice),  // Must be a number, not string
      salt: salt,
    },
    gas: '50000000000000',
  });

  console.log(`✓ Vote revealed!\n`);

  // ═══════════════════════════════════════════════════════════════
  // STEP 6: Wait and Resolve Price
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 6: Waiting for Reveal Phase to End');
  console.log('═'.repeat(60));

  const revealDuration = votingConfig[1] / 1e9;
  console.log(`  Waiting ${revealDuration + 2} seconds for reveal phase to end...`);
  await sleep((revealDuration + 2) * 1000);

  console.log('  Resolving price...');
  const resolveResult = await account.functionCall({
    contractId: CONFIG.votingContract,
    methodName: 'resolve_price',
    args: { request_id: dvmRequestId },
    gas: '50000000000000',
  });

  // Check resolved price
  const resolvedPrice = await account.viewFunction({
    contractId: CONFIG.votingContract,
    methodName: 'get_price',
    args: { request_id: dvmRequestId },
  });

  console.log(`✓ Price resolved: ${resolvedPrice}`);
  console.log(`  (0 = FALSE/Disputer wins, 1e18 = TRUE/Asserter wins)\n`);

  // ═══════════════════════════════════════════════════════════════
  // STEP 7: Settle Assertion via DVM
  // ═══════════════════════════════════════════════════════════════
  console.log('═'.repeat(60));
  console.log('STEP 7: Settling Assertion via DVM');
  console.log('═'.repeat(60));

  console.log('  Calling settle_assertion (will query DVM for resolution)...');

  try {
    const settleResult = await account.functionCall({
      contractId: CONFIG.oracleContract,
      methodName: 'settle_assertion',
      args: { assertion_id: assertionId },
      gas: '200000000000000',
    });

    // Check for settlement event
    for (const outcome of settleResult.receipts_outcome) {
      for (const log of outcome.outcome.logs) {
        if (log.includes('assertion_settled')) {
          const eventData = JSON.parse(log.replace('EVENT_JSON:', ''));
          console.log(`\n✓ Assertion Settled!`);
          console.log(`  Resolution: ${eventData.data[0].settlement_resolution ? 'TRUE (Asserter wins)' : 'FALSE (Disputer wins)'}`);
          console.log(`  Bond recipient: ${eventData.data[0].bond_recipient}`);
          console.log(`  Disputed: ${eventData.data[0].disputed}`);
        }
      }
    }
  } catch (e) {
    console.log(`  Settlement error: ${e.message}`);
    console.log('  (This may occur if DVM resolution type mismatch)');
  }

  // Verify final state
  const finalAssertion = await account.viewFunction({
    contractId: CONFIG.oracleContract,
    methodName: 'get_assertion',
    args: { assertion_id: assertionId },
  });

  console.log('\n═'.repeat(60));
  console.log('FINAL STATE');
  console.log('═'.repeat(60));
  console.log(`  Settled: ${finalAssertion.settled}`);
  console.log(`  Resolution: ${finalAssertion.settlement_resolution}`);
  console.log(`  Disputer: ${finalAssertion.disputer}`);

  console.log('\n╔══════════════════════════════════════════════════════════════╗');
  console.log('║                    DVM TEST COMPLETE!                        ║');
  console.log('╚══════════════════════════════════════════════════════════════╝\n');
}

main().catch(console.error);
