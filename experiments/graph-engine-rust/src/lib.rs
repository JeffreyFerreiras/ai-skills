#[cfg(test)]
extern crate self as graph_engine;

pub mod application;
pub mod compat;
pub mod domain;
pub mod infrastructure;

pub const ENGINE_VERSION: &str = "2.8.0";
pub const PHASE_DISPOSITIONS_ENGINE_VERSION: &str = "2.8.0";
pub const STATE_SCHEMA_VERSION: u32 = 6;
pub const EVIDENCE_SCHEMA_VERSION: u32 = 7;
pub const STABLE_ID_DOMAIN_VERSION: u32 = 5;
