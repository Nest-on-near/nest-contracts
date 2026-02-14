/**
 * View assertion details and status
 *
 * Usage: node view-assertion.js '[assertion_id_array]'
 */

import { connect, keyStores } from 'near-api-js';
import 'dotenv/config';

const CONFIG = {
  networkId: process.env.NETWORK_ID || 'testnet',
  nodeUrl: 'https://test.rpc.fastnear.com',
  oracleContract: process.env.ORACLE_CONTRACT || 'nest-oracle-3.testnet',
  tokenContract: process.env.TOKEN_CONTRACT || 'nest-token-1.testnet',
};

function bytesToHex(bytes) {
  return Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

function bytesToString(bytes) {
  return String.fromCharCode(...bytes.filter(b => b !== 0));
}

async function main() {
  const assertionIdArg = process.argv[2];

  const near = await connect({
    networkId: CONFIG.networkId,
    keyStore: new keyStores.InMemoryKeyStore(),
    nodeUrl: CONFIG.nodeUrl,
  });

  const account = await near.account('dontcare');

  // If no assertion ID provided, show oracle info
  if (!assertionIdArg) {
    console.log('NEST Oracle Status');
    console.log('==================\n');

    const [defaultCurrency, defaultIdentifier, defaultLiveness] = await Promise.all([
      account.viewFunction({
        contractId: CONFIG.oracleContract,
        methodName: 'default_currency',
        args: {},
      }),
      account.viewFunction({
        contractId: CONFIG.oracleContract,
        methodName: 'default_identifier',
        args: {},
      }),
      account.viewFunction({
        contractId: CONFIG.oracleContract,
        methodName: 'default_liveness',
        args: {},
      }),
    ]);

    console.log('Oracle Contract:', CONFIG.oracleContract);
    console.log('Token Contract:', CONFIG.tokenContract);
    console.log('Default Currency:', defaultCurrency);
    console.log('Default Identifier:', bytesToString(defaultIdentifier));
    console.log('Default Liveness:', defaultLiveness, 'ns', `(${Number(defaultLiveness) / 1e9 / 60} minutes)`);

    const minBond = await account.viewFunction({
      contractId: CONFIG.oracleContract,
      methodName: 'get_minimum_bond',
      args: { currency: CONFIG.tokenContract },
    });
    console.log('Minimum Bond:', minBond, `(${Number(minBond) / 1e24} tokens)`);

    const isWhitelisted = await account.viewFunction({
      contractId: CONFIG.oracleContract,
      methodName: 'is_currency_whitelisted',
      args: { currency: CONFIG.tokenContract },
    });
    console.log('Token Whitelisted:', isWhitelisted);

    console.log('\nUsage: node view-assertion.js \'[1,2,3,...,32]\'');
    return;
  }

  let assertionId;
  try {
    assertionId = JSON.parse(assertionIdArg);
  } catch (e) {
    console.error('Invalid assertion_id format. Expected JSON array.');
    process.exit(1);
  }

  console.log('Assertion Details');
  console.log('=================\n');

  const assertion = await account.viewFunction({
    contractId: CONFIG.oracleContract,
    methodName: 'get_assertion',
    args: { assertion_id: assertionId },
  });

  if (!assertion) {
    console.log('Assertion not found.');
    return;
  }

  console.log('Assertion ID:', bytesToHex(assertionId));
  console.log('Asserter:', assertion.asserter);
  console.log('Currency:', assertion.currency);
  console.log('Bond:', assertion.bond, `(${Number(assertion.bond) / 1e24} tokens)`);
  console.log('Identifier:', bytesToString(assertion.identifier));
  console.log('Domain ID:', bytesToHex(assertion.domain_id));

  const assertionTime = new Date(Number(assertion.assertion_time_ns) / 1e6);
  const expirationTime = new Date(Number(assertion.expiration_time_ns) / 1e6);

  console.log('\nTiming:');
  console.log('  Assertion Time:', assertionTime.toISOString());
  console.log('  Expiration Time:', expirationTime.toISOString());

  const now = Date.now();
  const expirationMs = Number(assertion.expiration_time_ns) / 1e6;
  if (now < expirationMs) {
    const remainingSec = Math.ceil((expirationMs - now) / 1000);
    console.log('  Time Remaining:', remainingSec, 'seconds');
  } else {
    console.log('  Status: LIVENESS EXPIRED');
  }

  console.log('\nState:');
  console.log('  Settled:', assertion.settled);
  console.log('  Settlement Resolution:', assertion.settlement_resolution);
  console.log('  Disputer:', assertion.disputer || 'None');
  console.log('  Callback Recipient:', assertion.callback_recipient || 'None');

  console.log('\nEscalation Manager Settings:');
  console.log('  Escalation Manager:', assertion.escalation_manager_settings.escalation_manager || 'None');
  console.log('  Asserting Caller:', assertion.escalation_manager_settings.asserting_caller);
  console.log('  Arbitrate via EM:', assertion.escalation_manager_settings.arbitrate_via_escalation_manager);
  console.log('  Discard Oracle:', assertion.escalation_manager_settings.discard_oracle);
  console.log('  Validate Disputers:', assertion.escalation_manager_settings.validate_disputers);
}

main().catch(console.error);
