//! A standard 52-card deck with shuffle and deal.

use crate::card::{Card, Suit, MAX_RANK, MIN_RANK};
use crate::rng::Rng;

pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    /// A fresh, ordered 52-card deck (call [`Deck::shuffle`] before dealing).
    pub fn standard() -> Deck {
        let mut cards = Vec::with_capacity(52);
        for suit in Suit::ALL {
            for rank in MIN_RANK..=MAX_RANK {
                cards.push(Card::new(rank, suit));
            }
        }
        Deck { cards }
    }

    pub fn shuffle(&mut self, rng: &mut Rng) {
        rng.shuffle(&mut self.cards);
    }

    /// Deal one card off the top, or `None` if the deck is exhausted.
    pub fn deal(&mut self) -> Option<Card> {
        self.cards.pop()
    }

    pub fn remaining(&self) -> usize {
        self.cards.len()
    }
}

impl Default for Deck {
    fn default() -> Self {
        Deck::standard()
    }
}
