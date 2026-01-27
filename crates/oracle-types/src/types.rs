//! Core type definitions for the Nest Optimistic Oracle.

/// A 32-byte fixed-size array used for identifiers, claims, and hashes.
///
/// This type is used throughout the oracle for:
/// - Assertion IDs (keccak256 hashes)
/// - Claims (encoded truth statements)
/// - Identifiers (e.g., ASSERT_TRUTH)
/// - Domain IDs (for grouping assertions)
///
/// Equivalent to `bytes32` in Solidity.
pub type Bytes32 = [u8; 32];
