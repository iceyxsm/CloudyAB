//! Core orchestration and layer routing for CloudyAB.
//!
//! This crate handles the decision logic for which layer to use (HTTP stealth vs
//! full browser), manages session lifecycle, and provides the trait interfaces
//! that each layer must implement.

pub mod config;
pub mod engine;
pub mod orchestrator;
pub mod router;
