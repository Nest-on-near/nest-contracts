/**
 * NEST Optimistic Oracle - Full Flow Test Script
 *
 * This script tests the complete oracle flow:
 * 1. Make an assertion (with bond)
 * 2. Dispute the assertion (with matching bond)
 * 3. Wait for DVM voting to resolve (or use owner fallback)
 * 4. Settle the disputed assertion
 *
 * The Oracle now has full DVM integration:
 * - When disputed, Oracle automatically calls voting.request_price()
 * - Settlement queries voting.get_price() for resolution
 * - Owner can still manually resolve as fallback
 *
 * Prerequisites:
 * - NEAR testnet account with some NEAR for gas
 * - NEST tokens for bonding
 * - Proper .env configuration
 *
 * Usage: node test-oracle-flow.js [dispute|undisputed]
 */

import { connect, keyStores, utils, Contract } from 'near-api-js';
import { readFileSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { createHash } from 'crypto';
import 'dotenv/config';

// Configuration
const CONFIG = {
  networkId: process.env.NETWORK_ID || 'testnet',
  nodeUrl: 'https://rpc.testnet.near.org',
  walletUrl: 'https://testnet.mynearwallet.com',
  helperUrl: 'https://helper.testnet.near.org',

  // Contract addresses
  oracleContract: process.env.ORACLE_CONTRACT || 'nest-oracle-3.testnet',
  tokenContract: process.env.TOKEN_CONTRACT || 'nest-token-1.testnet',
  votingContract: process.env.VOTING_CONTRACT || 'nest-voting-1.testnet',

  // Test account
  accountId: process.env.NEAR_ACCOUNT_ID,
  privateKey: process.env.NEAR_PRIVATE_KEY,
};

// Constants matching the Rust contract
const SCALE = BigInt('1000000000000000000'); // 1e18
const DEFAULT_LIVENESS_NS = BigInt('7200000000000'); // 2 hours in nanoseconds

/**
 * Convert a string to a 32-byte array (Bytes32)
 */
function stringToBytes32(str) {
  const bytes = new Array(32).fill(0);
  const strBytes = Buffer.from(str, 'utf8');
  for (let i = 0; i < Math.min(strBytes.length, 32); i++) {
    bytes[i] = strBytes[i];
  }
  return bytes;
}

/**
 * Convert a hex string to a byte array
 */
function hexToBytes(hex) {
  if (hex.startsWith('0x')) hex = hex.slice(2);
  const bytes = [];
  for (let i = 0; i < hex.length; i += 2) {
    bytes.push(parseInt(hex.substr(i, 2), 16));
  }
  return bytes;
}

/**
 * Convert bytes to hex string
 */
function bytesToHex(bytes) {
  return Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Load NEAR credentials from file or environment
 */
async function getKeyStore() {
  const keyStore = new keyStores.InMemoryKeyStore();

  if (CONFIG.privateKey) {
    // Use private key from environment
    const keyPair = utils.KeyPair.fromString(CONFIG.privateKey);
    await keyStore.setKey(CONFIG.networkId, CONFIG.accountId, keyPair);
    console.log(`✓ Loaded key from environment for ${CONFIG.accountId}`);
  } else {
    // Try to load from credentials file
    const credPath = join(homedir(), '.near-credentials', CONFIG.networkId, `${CONFIG.accountId}.json`);
    if (existsSync(credPath)) {
      const creds = JSON.parse(readFileSync(credPath, 'utf8'));
      const keyPair = utils.KeyPair.fromString(creds.private_key);
      await keyStore.setKey(CONFIG.networkId, CONFIG.accountId, keyPair);
      console.log(`✓ Loaded key from ${credPath}`);
    } else {
      throw new Error(`No credentials found. Set NEAR_PRIVATE_KEY or create credentials at ${credPath}`);
    }
  }

  return keyStore;
}

/**
 * Initialize NEAR connection and account
 */
async function initNear() {
  const keyStore = await getKeyStore();

  const near = await connect({
    networkId: CONFIG.networkId,
    keyStore,
    nodeUrl: CONFIG.nodeUrl,
  });

  const account = await near.account(CONFIG.accountId);
  console.log(`✓ Connected to NEAR ${CONFIG.networkId} as ${CONFIG.accountId}`);

  return { near, account };
}

/**
 * Get token balance for an account
 */
async function getTokenBalance(account, tokenContract, targetAccount) {
  const balance = await account.viewFunction({
    contractId: tokenContract,
    methodName: 'ft_balance_of',
    args: { account_id: targetAccount },
  });
  return BigInt(balance);
}

/**
 * Get minimum bond required for the oracle
 */
async function getMinimumBond(account, oracleContract, currency) {
  const minBond = await account.viewFunction({
    contractId: oracleContract,
    methodName: 'get_minimum_bond',
    args: { currency },
  });
  return BigInt(minBond);
}

/**
 * Get oracle default settings
 */
async function getOracleDefaults(account, oracleContract) {
  const [defaultCurrency, defaultIdentifier, defaultLiveness] = await Promise.all([
    account.viewFunction({
      contractId: oracleContract,
      methodName: 'default_currency',
      args: {},
    }),
    account.viewFunction({
      contractId: oracleContract,
      methodName: 'default_identifier',
      args: {},
    }),
    account.viewFunction({
      contractId: oracleContract,
      methodName: 'default_liveness',
      args: {},
    }),
  ]);

  return { defaultCurrency, defaultIdentifier, defaultLiveness };
}

/**
 * Get assertion details
 */
async function getAssertion(account, oracleContract, assertionId) {
  return await account.viewFunction({
    contractId: oracleContract,
    methodName: 'get_assertion',
    args: { assertion_id: assertionId },
  });
}

/**
 * Get the voting contract configured in the oracle
 */
async function getVotingContract(account, oracleContract) {
  return await account.viewFunction({
    contractId: oracleContract,
    methodName: 'get_voting_contract',
    args: {},
  });
}

/**
 * Get the DVM request ID for a disputed assertion
 */
async function getDisputeRequest(account, oracleContract, assertionId) {
  return await account.viewFunction({
    contractId: oracleContract,
    methodName: 'get_dispute_request',
    args: { assertion_id: assertionId },
  });
}

/**
 * Check if DVM has resolved the price for a request
 */
async function checkDvmResolution(account, votingContract, requestId) {
  try {
    const price = await account.viewFunction({
      contractId: votingContract,
      methodName: 'get_price',
      args: { request_id: requestId },
    });
    return { resolved: price !== null, price };
  } catch (e) {
    return { resolved: false, price: null, error: e.message };
  }
}

/**
 * Step 1: Make an assertion
 */
async function makeAssertion(account, bondAmount, claimText) {
  console.log('\n' + '='.repeat(60));
  console.log('STEP 1: Making an Assertion');
  console.log('='.repeat(60));

  const claim = stringToBytes32(claimText);
  const identifier = stringToBytes32('ASSERT_TRUTH');
  const domainId = stringToBytes32('TEST_DOMAIN');

  // Use shorter liveness for testing (5 minutes = 300 seconds = 300_000_000_000 ns)
  const testLivenessNs = '300000000000';

  const msg = {
    action: 'AssertTruth',
    claim: claim,
    asserter: CONFIG.accountId,
    identifier: identifier,
    domain_id: domainId,
    liveness_ns: testLivenessNs,
    callback_recipient: null,
    escalation_manager: null,
  };

  console.log(`Claim: "${claimText}"`);
  console.log(`Bond amount: ${bondAmount.toString()} (${Number(bondAmount) / 1e24} tokens)`);
  console.log(`Liveness: 5 minutes (for testing)`);

  const result = await account.functionCall({
    contractId: CONFIG.tokenContract,
    methodName: 'ft_transfer_call',
    args: {
      receiver_id: CONFIG.oracleContract,
      amount: bondAmount.toString(),
      msg: JSON.stringify(msg),
    },
    gas: '100000000000000', // 100 TGas
    attachedDeposit: '1', // 1 yoctoNEAR required for ft_transfer_call
  });

  // Parse the assertion ID from the result
  // The ft_on_transfer returns a JSON string containing the assertion_id
  let assertionId = null;

  // Look for the assertion_made event in the receipts
  for (const outcome of result.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      if (log.includes('assertion_made')) {
        try {
          const eventData = JSON.parse(log.replace('EVENT_JSON:', ''));
          if (eventData.event === 'assertion_made' && eventData.data?.[0]?.assertion_id) {
            assertionId = eventData.data[0].assertion_id;
          }
        } catch (e) {
          // Try parsing as raw JSON if EVENT_JSON prefix not found
          if (log.startsWith('{')) {
            try {
              const parsed = JSON.parse(log);
              if (parsed.assertion_id) {
                assertionId = parsed.assertion_id;
              }
            } catch (e2) {}
          }
        }
      }
    }
  }

  if (!assertionId) {
    // Try to get the return value from the function call
    const returnValue = result.status?.SuccessValue;
    if (returnValue) {
      const decoded = Buffer.from(returnValue, 'base64').toString('utf8');
      console.log('Return value:', decoded);
      try {
        const parsed = JSON.parse(decoded);
        if (parsed.assertion_id) {
          assertionId = parsed.assertion_id;
        }
      } catch (e) {}
    }
  }

  console.log(`✓ Assertion made!`);
  console.log(`Transaction: ${result.transaction.hash}`);

  if (assertionId) {
    console.log(`Assertion ID: ${JSON.stringify(assertionId)}`);
  } else {
    console.log('Note: Could not parse assertion ID from logs');
    console.log('Logs:', result.receipts_outcome.flatMap(r => r.outcome.logs));
  }

  return { result, assertionId };
}

/**
 * Step 2: Dispute an assertion
 */
async function disputeAssertion(account, assertionId, bondAmount) {
  console.log('\n' + '='.repeat(60));
  console.log('STEP 2: Disputing the Assertion');
  console.log('='.repeat(60));

  const msg = {
    action: 'DisputeAssertion',
    assertion_id: assertionId,
    disputer: CONFIG.accountId,
  };

  console.log(`Assertion ID: ${JSON.stringify(assertionId)}`);
  console.log(`Bond amount: ${bondAmount.toString()}`);

  const result = await account.functionCall({
    contractId: CONFIG.tokenContract,
    methodName: 'ft_transfer_call',
    args: {
      receiver_id: CONFIG.oracleContract,
      amount: bondAmount.toString(),
      msg: JSON.stringify(msg),
    },
    gas: '100000000000000', // 100 TGas
    attachedDeposit: '1', // 1 yoctoNEAR
  });

  console.log(`✓ Assertion disputed!`);
  console.log(`Transaction: ${result.transaction.hash}`);

  // Check for dispute event
  for (const outcome of result.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      if (log.includes('assertion_disputed')) {
        console.log('Event:', log);
      }
    }
  }

  return result;
}

/**
 * Step 3a: Try to settle via DVM (the normal flow)
 * This calls settle_assertion which queries DVM for resolution
 */
async function settleViaDvm(account, assertionId) {
  console.log('\n' + '='.repeat(60));
  console.log('STEP 3: Settling via DVM');
  console.log('='.repeat(60));

  console.log(`Assertion ID: ${JSON.stringify(assertionId)}`);
  console.log('Calling settle_assertion (will query DVM for resolution)...');

  const result = await account.functionCall({
    contractId: CONFIG.oracleContract,
    methodName: 'settle_assertion',
    args: {
      assertion_id: assertionId,
    },
    gas: '200000000000000', // 200 TGas (needs more for cross-contract calls)
  });

  console.log(`✓ Settlement initiated!`);
  console.log(`Transaction: ${result.transaction.hash}`);

  // Check for settlement event
  for (const outcome of result.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      if (log.includes('assertion_settled')) {
        console.log('Event:', log);
      }
    }
  }

  return result;
}

/**
 * Step 3b: Settle a disputed assertion via owner fallback
 * This is only used when DVM is not available or for testing
 */
async function settleDisputedAssertion(account, assertionId, resolution) {
  console.log('\n' + '='.repeat(60));
  console.log('STEP 3 (Fallback): Owner Manual Resolution');
  console.log('='.repeat(60));

  console.log(`Assertion ID: ${JSON.stringify(assertionId)}`);
  console.log(`Resolution: ${resolution} (${resolution ? 'Asserter wins' : 'Disputer wins'})`);

  const result = await account.functionCall({
    contractId: CONFIG.oracleContract,
    methodName: 'resolve_disputed_assertion',
    args: {
      assertion_id: assertionId,
      resolution: resolution,
    },
    gas: '100000000000000', // 100 TGas
  });

  console.log(`✓ Assertion settled!`);
  console.log(`Transaction: ${result.transaction.hash}`);

  // Check for settlement event
  for (const outcome of result.receipts_outcome) {
    for (const log of outcome.outcome.logs) {
      if (log.includes('assertion_settled')) {
        console.log('Event:', log);
      }
    }
  }

  return result;
}

/**
 * Settle an undisputed assertion (after liveness expires)
 */
async function settleUndisputedAssertion(account, assertionId) {
  console.log('\n' + '='.repeat(60));
  console.log('STEP 3 (Alt): Settling Undisputed Assertion');
  console.log('='.repeat(60));

  console.log(`Assertion ID: ${JSON.stringify(assertionId)}`);
  console.log('Note: This only works after the liveness period has expired');

  const result = await account.functionCall({
    contractId: CONFIG.oracleContract,
    methodName: 'settle_assertion',
    args: {
      assertion_id: assertionId,
    },
    gas: '100000000000000', // 100 TGas
  });

  console.log(`✓ Assertion settled!`);
  console.log(`Transaction: ${result.transaction.hash}`);

  return result;
}

/**
 * Main test flow
 */
async function main() {
  console.log('╔══════════════════════════════════════════════════════════╗');
  console.log('║     NEST Optimistic Oracle - Full Flow Test              ║');
  console.log('╚══════════════════════════════════════════════════════════╝\n');

  // Validate configuration
  if (!CONFIG.accountId) {
    console.error('Error: NEAR_ACCOUNT_ID not set in .env');
    console.log('Create a .env file based on .env.example');
    process.exit(1);
  }

  // Initialize NEAR connection
  const { near, account } = await initNear();

  // Get oracle defaults
  console.log('\nFetching oracle configuration...');
  const defaults = await getOracleDefaults(account, CONFIG.oracleContract);
  console.log('Default currency:', defaults.defaultCurrency);
  console.log('Default identifier:', bytesToHex(defaults.defaultIdentifier));
  console.log('Default liveness:', defaults.defaultLiveness, 'ns');

  // Get minimum bond
  const minBond = await getMinimumBond(account, CONFIG.oracleContract, CONFIG.tokenContract);
  console.log('Minimum bond:', minBond.toString(), `(${Number(minBond) / 1e24} tokens)`);

  // Check token balance
  const balance = await getTokenBalance(account, CONFIG.tokenContract, CONFIG.accountId);
  console.log('Your token balance:', balance.toString(), `(${Number(balance) / 1e24} tokens)`);

  // Need at least 2x minBond for assert + dispute
  const requiredBalance = minBond * 2n;
  if (balance < requiredBalance) {
    console.error(`\nError: Insufficient token balance.`);
    console.error(`Required: ${requiredBalance.toString()} (for assertion + dispute)`);
    console.error(`Have: ${balance.toString()}`);
    console.log('\nPlease acquire more tokens and try again.');
    process.exit(1);
  }

  // Ask user which flow to test
  const args = process.argv.slice(2);
  const flowType = args[0] || 'dispute'; // 'dispute' or 'undisputed'

  console.log(`\nRunning ${flowType === 'dispute' ? 'DISPUTED' : 'UNDISPUTED'} flow test...\n`);

  // Step 1: Make assertion
  const claimText = `Test claim at ${new Date().toISOString()}`;
  const { assertionId } = await makeAssertion(account, minBond, claimText);

  if (!assertionId) {
    console.error('\nCould not retrieve assertion ID. Check transaction logs.');
    process.exit(1);
  }

  // Get assertion details
  console.log('\nFetching assertion details...');
  const assertion = await getAssertion(account, CONFIG.oracleContract, assertionId);
  console.log('Assertion:', JSON.stringify(assertion, null, 2));

  if (flowType === 'dispute') {
    // Step 2: Dispute
    await disputeAssertion(account, assertionId, minBond);

    // Get updated assertion
    const updatedAssertion = await getAssertion(account, CONFIG.oracleContract, assertionId);
    console.log('\nAssertion after dispute:', JSON.stringify(updatedAssertion, null, 2));

    // Check if DVM request was created
    console.log('\nChecking DVM escalation status...');
    const votingContract = await getVotingContract(account, CONFIG.oracleContract);
    console.log('Voting contract:', votingContract);

    const dvmRequestId = await getDisputeRequest(account, CONFIG.oracleContract, assertionId);
    if (dvmRequestId) {
      console.log('DVM Request ID:', bytesToHex(dvmRequestId));

      // Check if DVM has resolved
      const dvmStatus = await checkDvmResolution(account, CONFIG.votingContract, dvmRequestId);
      console.log('DVM Resolution status:', dvmStatus);

      if (dvmStatus.resolved) {
        // Try to settle via DVM
        console.log('\nDVM has resolved! Settling via DVM...');
        try {
          await settleViaDvm(account, assertionId);
        } catch (e) {
          console.log('Settlement via DVM failed:', e.message);
        }
      } else {
        console.log('\nDVM has not resolved yet. The dispute requires voter participation.');
        console.log('Steps to resolve:');
        console.log('  1. Voters commit votes during commit phase');
        console.log('  2. Voters reveal votes during reveal phase');
        console.log('  3. Call voting.resolve_price() after reveal phase');
        console.log('  4. Call oracle.settle_assertion() to distribute bonds');
      }
    } else {
      console.log('No DVM request found (voting contract may not be configured)');
    }

    // Step 3: Try owner fallback if DVM not available
    console.log('\nAttempting owner fallback settlement...');
    try {
      await settleDisputedAssertion(account, assertionId, false);
    } catch (e) {
      console.log('\nNote: Owner fallback failed - this is expected if you are not the oracle owner.');
      console.log('Use DVM voting flow to resolve disputes.');
      console.log('Error:', e.message);
    }
  } else {
    // Undisputed flow - wait for liveness then settle
    console.log('\nFor undisputed settlement, you need to wait for the liveness period to expire.');
    console.log('Run the following command after 5 minutes:');
    console.log(`  node settle-assertion.js ${JSON.stringify(assertionId)}`);
  }

  console.log('\n' + '='.repeat(60));
  console.log('Test completed!');
  console.log('='.repeat(60));
}

// Run the main function
main().catch(console.error);
