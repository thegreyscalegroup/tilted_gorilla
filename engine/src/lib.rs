//! Tilted Gorilla — Texas Hold'em rules engine.
//!
//! Pure, dependency-free game logic: cards, deck, hand evaluation, and (added
//! in later milestones) the betting-round state machine. Nothing here knows
//! about UI, WASM, or opponents — the AI and web crates build on top of it.

pub mod card;
pub mod deck;
pub mod describe;
pub mod eval;
pub mod hand;
pub mod pot;
pub mod rng;

pub use card::{Card, Suit};
pub use deck::Deck;
pub use describe::{describe, describe_hole, draws};
pub use eval::{eval5, eval7, Category, HandValue};
pub use hand::{Action, Hand, Legal, Payouts, Seat, SeatStatus, Street};
pub use pot::{build_pots, Pot};
pub use rng::Rng;

#[cfg(test)]
mod tests;
