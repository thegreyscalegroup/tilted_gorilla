//! Tilted Gorilla — poker AI.
//!
//! Monte-Carlo equity ([`equity`]) feeding a tunable pot-odds policy
//! ([`decide`]) with three difficulty [`Tier`]s. Depends only on the rules
//! engine; knows nothing about UI or WASM.

pub mod equity;
pub mod policy;

pub use equity::equity;
pub use policy::{decide, Tier};

#[cfg(test)]
mod tests;
