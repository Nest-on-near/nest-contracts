//! Shared types and interfaces for the Nest Optimistic Oracle.
//!
//! This crate provides common type definitions, traits, and events used across
//! the Nest oracle ecosystem. It enables type-safe interactions between the
//! oracle contract and integrating contracts.
//!
//! # Modules
//!
//! - [`events`] - NEP-297 compliant event definitions for indexing
//! - [`interfaces`] - Trait definitions for oracle and callback contracts
//! - [`types`] - Core type aliases and definitions

pub mod events;
pub mod interfaces;
pub mod types;
