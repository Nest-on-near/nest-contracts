//! Interface definitions for the Nest Optimistic Oracle.
//!
//! This module contains trait definitions and data structures that define
//! the contract interfaces for the oracle ecosystem.

pub mod callback_recipient;
pub mod escalation_manager;
pub mod optimistic_oracle;

pub use callback_recipient::*;
pub use escalation_manager::*;
pub use optimistic_oracle::*;
