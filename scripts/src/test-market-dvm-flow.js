#!/usr/bin/env node
/**
 * End-to-end market + oracle + DVM voting flow runner using NEAR CLI.
 *
 * Flow this script verifies:
 * 1) Market setup:
 *    - Creates a new market using USDC `ft_transfer_call` (CreateMarket).
 *    - Executes YES and NO buys to seed trade activity and pool state.
 *    - Waits until market reports `is_resolvable_now = true`.
 *
 * 2) Resolution submission:
 *    - Submits `SubmitResolution` with bond via USDC `ft_transfer_call`.
 *    - Reads `market.active_assertion_id` from `get_resolution_status`.
 *    - Confirms oracle actually stores this assertion (`oracle.get_assertion`).
 *
 * 3) Branch A (optional): Undisputed settlement
 *    - Waits for assertion liveness expiry.
 *    - Calls `oracle.settle_assertion`.
 *    - Verifies final market state via `get_market`.
 *
 * 4) Branch B (default): Disputed + DVM voting
 *    - Disputer submits bond to oracle (`DisputeAssertion`).
 *    - Waits for oracle -> DVM mapping (`get_dispute_request`).
 *    - Two voters commit votes by staking voting tokens through
 *      `voting-token.ft_transfer_call(CommitVote)`.
 *    - Advances to reveal phase, reveals both votes, resolves DVM price.
 *    - Calls `oracle.settle_assertion` and verifies market settles with
 *      expected status/outcome/prices.
 *
 * 5) Final checks/output:
 *    - Prints request id, resolved DVM price, market status, outcome, and prices.
 *    - Fails fast on missing env/config and key oracle handshake failures.
 *
 * Optional:
 * - Fast voting durations (owner-only on voting contract)
 * - Undisputed branch (waits oracle liveness, ~2h with current market contract)
 */

import { execFileSync } from 'node:child_process';
import { randomBytes, createHash } from 'node:crypto';
import 'dotenv/config';

const REQUIRED_ENV = [
  'MARKET_CONTRACT',
  'ORACLE_CONTRACT',
  'VOTING_CONTRACT',
  'OUTCOME_TOKEN_CONTRACT',
  'USDC_CONTRACT',
  'VOTING_TOKEN_CONTRACT',
  'CREATOR_ACCOUNT',
  'TRADER_YES_ACCOUNT',
  'TRADER_NO_ACCOUNT',
  'DISPUTER_ACCOUNT',
  'VOTER1_ACCOUNT',
  'VOTER2_ACCOUNT',
];

const CONFIG = {
  network: process.env.NETWORK ?? 'testnet',
  market: process.env.MARKET_CONTRACT,
  oracle: process.env.ORACLE_CONTRACT,
  voting: process.env.VOTING_CONTRACT,
  outcomeToken: process.env.OUTCOME_TOKEN_CONTRACT,
  usdc: process.env.USDC_CONTRACT,
  votingToken: process.env.VOTING_TOKEN_CONTRACT,
  creator: process.env.CREATOR_ACCOUNT,
  traderYes: process.env.TRADER_YES_ACCOUNT,
  traderNo: process.env.TRADER_NO_ACCOUNT,
  disputer: process.env.DISPUTER_ACCOUNT,
  voter1: process.env.VOTER1_ACCOUNT,
  voter2: process.env.VOTER2_ACCOUNT,

  question: process.env.MARKET_QUESTION ?? `DVM flow test ${new Date().toISOString()}`,
  description: process.env.MARKET_DESCRIPTION ?? 'automated test flow',

  createLiquidity: process.env.CREATE_LIQUIDITY_USDC_UNITS ?? '100000000', // 100 USDC (6d)
  buyYesAmount: process.env.BUY_YES_USDC_UNITS ?? '20000000', // 20 USDC
  buyNoAmount: process.env.BUY_NO_USDC_UNITS ?? '20000000', // 20 USDC
  buyMinOut: process.env.BUY_MIN_TOKENS_OUT ?? '1',

  resolveBondAmount: process.env.RESOLUTION_BOND_USDC_UNITS, // optional; defaults to oracle min bond
  disputeBondAmount: process.env.DISPUTE_BOND_USDC_UNITS, // optional; defaults to resolution bond

  marketResolutionDelaySec: Number(process.env.MARKET_RESOLUTION_DELAY_SEC ?? '90'),
  pollMs: Number(process.env.POLL_MS ?? '5000'),

  setFastVoting: process.env.SET_FAST_VOTING === '1',
  votingOwner: process.env.VOTING_OWNER_ACCOUNT ?? process.env.ORACLE_CONTRACT,
  commitDurationNs: process.env.COMMIT_DURATION_NS ?? '120000000000', // 120s
  revealDurationNs: process.env.REVEAL_DURATION_NS ?? '120000000000', // 120s

  runUndisputedBranch: process.env.RUN_UNDISPUTED_BRANCH === '1',
  undisputedSettler: process.env.UNDISPUTED_SETTLER_ACCOUNT ?? process.env.CREATOR_ACCOUNT,

  voter1Price: process.env.VOTER1_PRICE ?? '1000000000000000000', // TRUE (1e18)
  voter2Price: process.env.VOTER2_PRICE ?? '1000000000000000000', // TRUE (1e18)
  voter1Stake: process.env.VOTER1_STAKE_UNITS ?? '1000000000000000000000000', // 1 token @24d
  voter2Stake: process.env.VOTER2_STAKE_UNITS ?? '1000000000000000000000000',
};

function assertEnv() {
  const missing = REQUIRED_ENV.filter((k) => !process.env[k]);
  if (missing.length) {
    throw new Error(`Missing required env vars: ${missing.join(', ')}`);
  }
}

function nowNs() {
  return BigInt(Date.now()) * 1_000_000n;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function near(args, { expectJson = false } = {}) {
  const out = execFileSync('near', ['--quiet', ...args], {
    encoding: 'utf8',
    stdio: ['pipe', 'pipe', 'pipe'],
  }).trim();

  if (!expectJson) return out;

  const lines = out
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  const last = lines.at(-1) ?? '';
  try {
    return JSON.parse(last);
  } catch {
    if (last === 'true') return true;
    if (last === 'false') return false;
    if (/^-?\d+$/.test(last)) return Number(last);
    if (last === 'null') return null;
    return last;
  }
}

function view(contractId, method, args = {}) {
  return near(
    [
      'contract',
      'call-function',
      'as-read-only',
      contractId,
      method,
      'json-args',
      JSON.stringify(args),
      'network-config',
      CONFIG.network,
      'now',
    ],
    { expectJson: true }
  );
}

function tx(contractId, method, args, signAs, { gas = '150 Tgas', deposit = '0 NEAR' } = {}) {
  console.log(`\n> ${signAs} :: ${contractId}.${method}`);
  return near([
    'contract',
    'call-function',
    'as-transaction',
    contractId,
    method,
    'json-args',
    JSON.stringify(args),
    'prepaid-gas',
    gas,
    'attached-deposit',
    deposit,
    'sign-as',
    signAs,
    'network-config',
    CONFIG.network,
    'sign-with-keychain',
    'send',
  ]);
}

function hexToBytes(hex) {
  const clean = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (clean.length !== 64) throw new Error(`Expected 32-byte hex, got length=${clean.length}`);
  const out = [];
  for (let i = 0; i < clean.length; i += 2) out.push(parseInt(clean.slice(i, i + 2), 16));
  return out;
}

function i128ToLe16Bytes(price) {
  let v = BigInt(price);
  if (v < 0n) v += 1n << 128n;
  const out = Buffer.alloc(16);
  for (let i = 0; i < 16; i += 1) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

function computeCommitHash(price, saltBytes32) {
  const payload = Buffer.concat([i128ToLe16Bytes(price), Buffer.from(saltBytes32)]);
  return Array.from(createHash('sha256').update(payload).digest());
}

async function waitForResolvableMarket(marketId) {
  for (;;) {
    const rs = view(CONFIG.market, 'get_resolution_status', { market_id: marketId });
    if (rs?.is_resolvable_now) return rs;
    process.stdout.write('.');
    await sleep(CONFIG.pollMs);
  }
}

async function waitForRequestPhase(requestId, wantedPhase) {
  for (;;) {
    const req = view(CONFIG.voting, 'get_request', { request_id: requestId });
    if (req?.phase === wantedPhase) return req;
    process.stdout.write('.');
    await sleep(CONFIG.pollMs);
  }
}

async function waitForAssertionExpiry(assertionBytes) {
  for (;;) {
    const assertion = view(CONFIG.oracle, 'get_assertion', { assertion_id: assertionBytes });
    if (!assertion) throw new Error('Assertion not found while waiting for expiry');
    const expiry = BigInt(assertion.expiration_time_ns);
    if (nowNs() >= expiry) return assertion;
    process.stdout.write('.');
    await sleep(CONFIG.pollMs);
  }
}

async function waitForDisputeRequest(assertionBytes) {
  for (;;) {
    const reqId = view(CONFIG.oracle, 'get_dispute_request', { assertion_id: assertionBytes });
    if (Array.isArray(reqId) && reqId.length === 32) return reqId;
    process.stdout.write('.');
    await sleep(CONFIG.pollMs);
  }
}

async function createMarketAndTrade() {
  const beforeCount = Number(view(CONFIG.market, 'get_market_count', {}));
  const marketId = beforeCount;
  const resolutionTimeNs = (nowNs() + BigInt(CONFIG.marketResolutionDelaySec) * 1_000_000_000n).toString();

  tx(
    CONFIG.usdc,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.market,
      amount: CONFIG.createLiquidity,
      msg: JSON.stringify({
        action: 'CreateMarket',
        question: CONFIG.question,
        description: CONFIG.description,
        resolution_time_ns: resolutionTimeNs,
      }),
    },
    CONFIG.creator,
    { gas: '150 Tgas', deposit: '1 yoctoNEAR' }
  );

  tx(
    CONFIG.usdc,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.market,
      amount: CONFIG.buyYesAmount,
      msg: JSON.stringify({
        action: 'Buy',
        market_id: marketId,
        outcome: 'Yes',
        min_tokens_out: CONFIG.buyMinOut,
      }),
    },
    CONFIG.traderYes,
    { gas: '120 Tgas', deposit: '1 yoctoNEAR' }
  );

  tx(
    CONFIG.usdc,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.market,
      amount: CONFIG.buyNoAmount,
      msg: JSON.stringify({
        action: 'Buy',
        market_id: marketId,
        outcome: 'No',
        min_tokens_out: CONFIG.buyMinOut,
      }),
    },
    CONFIG.traderNo,
    { gas: '120 Tgas', deposit: '1 yoctoNEAR' }
  );

  console.log(`\nWaiting for market ${marketId} to become resolvable`);
  await waitForResolvableMarket(marketId);
  console.log('\nMarket is resolvable');
  return marketId;
}

function submitResolution(marketId) {
  const minBond = String(view(CONFIG.oracle, 'get_minimum_bond', { currency: CONFIG.usdc }));
  const bond = CONFIG.resolveBondAmount ?? minBond;

  const submitTxOut = tx(
    CONFIG.usdc,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.market,
      amount: bond,
      msg: JSON.stringify({
        action: 'SubmitResolution',
        market_id: marketId,
        outcome: 'Yes',
      }),
    },
    CONFIG.creator,
    { gas: '180 Tgas', deposit: '1 yoctoNEAR' }
  );
  console.log(`[DIAG] submitResolution tx output:\n${submitTxOut}`);

  const rs = view(CONFIG.market, 'get_resolution_status', { market_id: marketId });
  const assertionHex = rs?.active_assertion_id;
  if (!assertionHex) throw new Error(`No active assertion id for market ${marketId}`);

  // ── Diagnostic: confirm oracle received the assertion ──
  const assertionBytes = hexToBytes(assertionHex);
  console.log(`\n[DIAG] market.active_assertion_id (hex): ${assertionHex}`);
  console.log(`[DIAG] market resolution_status:`, JSON.stringify(rs, null, 2));

  const oracleAssertion = view(CONFIG.oracle, 'get_assertion', { assertion_id: assertionBytes });
  console.log(`[DIAG] oracle.get_assertion(${assertionHex}):`, JSON.stringify(oracleAssertion, null, 2));

  if (!oracleAssertion) {
    console.error(`\n[DIAG] *** CONFIRMED: Oracle does NOT have this assertion ***`);
    console.error(`[DIAG] The ft_transfer_call from market->USDC->oracle likely failed silently.`);
    console.error(`[DIAG] Market is stuck in Resolving with a phantom assertion_id.`);

    // Check USDC balances to see if bond was refunded
    const marketBalance = view(CONFIG.usdc, 'ft_balance_of', { account_id: CONFIG.market });
    const oracleBalance = view(CONFIG.usdc, 'ft_balance_of', { account_id: CONFIG.oracle });
    const creatorBalance = view(CONFIG.usdc, 'ft_balance_of', { account_id: CONFIG.creator });
    console.error(`[DIAG] USDC balances after submit:`);
    console.error(`[DIAG]   market  (${CONFIG.market}): ${marketBalance}`);
    console.error(`[DIAG]   oracle  (${CONFIG.oracle}): ${oracleBalance}`);
    console.error(`[DIAG]   creator (${CONFIG.creator}): ${creatorBalance}`);
    console.error(`[DIAG] If oracle balance did NOT increase by ${bond}, the bond was refunded.`);
  } else {
    console.log(`[DIAG] ✓ Oracle has the assertion. asserter=${oracleAssertion.asserter}, bond=${oracleAssertion.bond}, expires=${oracleAssertion.expiration_time_ns}`);
  }
  // ── End Diagnostic ──

  return { assertionHex, bond };
}

async function runUndisputedBranch(marketId, assertionHex) {
  console.log(`\n[Undisputed] waiting for assertion expiry for market ${marketId}`);
  const assertionBytes = hexToBytes(assertionHex);
  await waitForAssertionExpiry(assertionBytes);
  console.log('\n[Undisputed] settling assertion');
  tx(
    CONFIG.oracle,
    'settle_assertion',
    { assertion_id: assertionBytes },
    CONFIG.undisputedSettler,
    { gas: '180 Tgas', deposit: '0 NEAR' }
  );
  const market = view(CONFIG.market, 'get_market', { market_id: marketId });
  return market;
}

async function runDisputedBranch(marketId, assertionHex, bondAmount) {
  const assertionBytes = hexToBytes(assertionHex);

  // ── Pre-dispute diagnostic ──
  const preDisputeAssertion = view(CONFIG.oracle, 'get_assertion', { assertion_id: assertionBytes });
  if (!preDisputeAssertion) {
    console.error(`\n[DIAG] *** ABORT: Oracle has no assertion ${assertionHex} — dispute will fail ***`);
    console.error(`[DIAG] Skipping dispute to avoid infinite poll.`);
    return { market: view(CONFIG.market, 'get_market', { market_id: marketId }), aborted: true };
  }
  console.log(`[DIAG] Pre-dispute oracle assertion OK: settled=${preDisputeAssertion.settled}, disputer=${preDisputeAssertion.disputer}`);
  // ── End pre-dispute diagnostic ──

  tx(
    CONFIG.usdc,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.oracle,
      amount: CONFIG.disputeBondAmount ?? bondAmount,
      msg: JSON.stringify({
        action: 'DisputeAssertion',
        assertion_id: assertionBytes,
        disputer: CONFIG.disputer,
      }),
    },
    CONFIG.disputer,
    { gas: '200 Tgas', deposit: '1 yoctoNEAR' }
  );

  console.log('\nWaiting for oracle -> DVM dispute request mapping');
  const requestId = await waitForDisputeRequest(assertionBytes);
  console.log('\nDVM request_id acquired');

  const voter1Salt = Array.from(randomBytes(32));
  const voter2Salt = Array.from(randomBytes(32));
  const voter1Hash = computeCommitHash(CONFIG.voter1Price, voter1Salt);
  const voter2Hash = computeCommitHash(CONFIG.voter2Price, voter2Salt);

  tx(
    CONFIG.votingToken,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.voting,
      amount: CONFIG.voter1Stake,
      msg: JSON.stringify({
        action: 'CommitVote',
        request_id: requestId,
        commit_hash: voter1Hash,
      }),
    },
    CONFIG.voter1,
    { gas: '200 Tgas', deposit: '1 yoctoNEAR' }
  );

  tx(
    CONFIG.votingToken,
    'ft_transfer_call',
    {
      receiver_id: CONFIG.voting,
      amount: CONFIG.voter2Stake,
      msg: JSON.stringify({
        action: 'CommitVote',
        request_id: requestId,
        commit_hash: voter2Hash,
      }),
    },
    CONFIG.voter2,
    { gas: '200 Tgas', deposit: '1 yoctoNEAR' }
  );

  console.log('\nWaiting until commit phase can advance to reveal');
  await waitForRequestPhase(requestId, 'Commit');
  for (;;) {
    try {
      tx(
        CONFIG.voting,
        'advance_to_reveal',
        { request_id: requestId },
        CONFIG.creator,
        { gas: '100 Tgas', deposit: '0 NEAR' }
      );
      break;
    } catch {
      process.stdout.write('.');
      await sleep(CONFIG.pollMs);
    }
  }

  // reveal_vote expects `price: i128` as a raw JSON number (not string-wrapped).
  // 1e18 exceeds Number.MAX_SAFE_INTEGER, so we build the JSON manually to avoid
  // JSON.stringify quoting it or losing precision.
  function revealVoteArgs(requestId, price, salt) {
    return `{"request_id":${JSON.stringify(requestId)},"price":${price},"salt":${JSON.stringify(salt)}}`;
  }

  near([
    'contract', 'call-function', 'as-transaction',
    CONFIG.voting, 'reveal_vote', 'json-args',
    revealVoteArgs(requestId, CONFIG.voter1Price, voter1Salt),
    'prepaid-gas', '120 Tgas', 'attached-deposit', '0 NEAR',
    'sign-as', CONFIG.voter1,
    'network-config', CONFIG.network, 'sign-with-keychain', 'send',
  ]);

  near([
    'contract', 'call-function', 'as-transaction',
    CONFIG.voting, 'reveal_vote', 'json-args',
    revealVoteArgs(requestId, CONFIG.voter2Price, voter2Salt),
    'prepaid-gas', '120 Tgas', 'attached-deposit', '0 NEAR',
    'sign-as', CONFIG.voter2,
    'network-config', CONFIG.network, 'sign-with-keychain', 'send',
  ]);

  console.log('\nWaiting until reveal phase can resolve');
  await waitForRequestPhase(requestId, 'Reveal');
  for (;;) {
    try {
      tx(
        CONFIG.voting,
        'resolve_price',
        { request_id: requestId },
        CONFIG.creator,
        { gas: '180 Tgas', deposit: '0 NEAR' }
      );
      break;
    } catch {
      process.stdout.write('.');
      await sleep(CONFIG.pollMs);
    }
  }

  tx(
    CONFIG.oracle,
    'settle_assertion',
    { assertion_id: assertionBytes },
    CONFIG.creator,
    { gas: '200 Tgas', deposit: '0 NEAR' }
  );

  const market = view(CONFIG.market, 'get_market', { market_id: marketId });
  const dvmRequest = view(CONFIG.voting, 'get_request', { request_id: requestId });
  const resolvedPrice = view(CONFIG.voting, 'get_price', { request_id: requestId });
  const prices = view(CONFIG.market, 'get_prices', { market_id: marketId });
  return { market, requestId, dvmRequest, resolvedPrice, prices };
}

async function main() {
  assertEnv();

  console.log('\n=== NEST Market + DVM E2E Runner ===');
  console.log(`Network: ${CONFIG.network}`);
  console.log(`Market:  ${CONFIG.market}`);
  console.log(`Oracle:  ${CONFIG.oracle}`);
  console.log(`Voting:  ${CONFIG.voting}`);

  if (CONFIG.setFastVoting) {
    tx(
      CONFIG.voting,
      'set_commit_phase_duration',
      { duration_ns: Number(CONFIG.commitDurationNs) },
      CONFIG.votingOwner,
      { gas: '30 Tgas', deposit: '0 NEAR' }
    );
    tx(
      CONFIG.voting,
      'set_reveal_phase_duration',
      { duration_ns: Number(CONFIG.revealDurationNs) },
      CONFIG.votingOwner,
      { gas: '30 Tgas', deposit: '0 NEAR' }
    );
  }

  if (CONFIG.runUndisputedBranch) {
    console.log('\n--- Branch A: Undisputed settlement ---');
    const marketId = await createMarketAndTrade();
    const { assertionHex } = submitResolution(marketId);
    const finalMarket = await runUndisputedBranch(marketId, assertionHex);
    console.log('\nUndisputed result:');
    console.log(JSON.stringify(finalMarket, null, 2));
  }

  console.log('\n--- Branch B: Disputed + DVM commit/reveal ---');
  const marketId = await createMarketAndTrade();
  const { assertionHex, bond } = submitResolution(marketId);
  const out = await runDisputedBranch(marketId, assertionHex, bond);

  console.log('\nDisputed result summary:');
  console.log(
    JSON.stringify(
      {
        market_id: marketId,
        request_id: out.requestId,
        resolved_price: out.resolvedPrice,
        market_status: out.market?.status,
        market_outcome: out.market?.outcome,
        market_prices: out.prices,
      },
      null,
      2
    )
  );
}

main().catch((err) => {
  console.error('\nE2E flow failed:\n', err?.message ?? err);
  process.exit(1);
});
