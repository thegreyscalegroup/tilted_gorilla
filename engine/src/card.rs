//! Cards, ranks, and suits.
//!
//! Ranks are stored as `u8` in `2..=14`, where 11=J, 12=Q, 13=K, 14=A. Keeping
//! ranks as plain numbers makes straight detection and kicker comparison trivial.

use core::fmt;

/// The four suits. Order is arbitrary and never affects hand strength in poker.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    /// Single-character symbol, e.g. for compact rendering.
    pub fn symbol(self) -> char {
        match self {
            Suit::Clubs => '♣',
            Suit::Diamonds => '♦',
            Suit::Hearts => '♥',
            Suit::Spades => '♠',
        }
    }
}

/// Lowest and highest rank values (ace high).
pub const MIN_RANK: u8 = 2;
pub const MAX_RANK: u8 = 14;

/// A single playing card. `rank` is in `2..=14`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Card {
    pub rank: u8,
    pub suit: Suit,
}

impl Card {
    pub fn new(rank: u8, suit: Suit) -> Card {
        debug_assert!(
            (MIN_RANK..=MAX_RANK).contains(&rank),
            "rank {rank} out of range"
        );
        Card { rank, suit }
    }

    /// Parse a two-char code like `"As"`, `"Td"`, `"9h"` (rank then suit
    /// letter, case-insensitive). Returns `None` on anything malformed. Handy
    /// for tests, fixtures, and any text-based board input.
    pub fn parse(code: &str) -> Option<Card> {
        let mut chars = code.chars();
        let r = chars.next()?;
        let s = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        let rank = match r.to_ascii_uppercase() {
            '2'..='9' => r.to_digit(10)? as u8,
            'T' => 10,
            'J' => 11,
            'Q' => 12,
            'K' => 13,
            'A' => 14,
            _ => return None,
        };
        let suit = match s.to_ascii_lowercase() {
            'c' => Suit::Clubs,
            'd' => Suit::Diamonds,
            'h' => Suit::Hearts,
            's' => Suit::Spades,
            _ => return None,
        };
        Some(Card::new(rank, suit))
    }

    /// The letter/number used when displaying the rank (T for ten, A/K/Q/J).
    pub fn rank_label(self) -> &'static str {
        match self.rank {
            2 => "2",
            3 => "3",
            4 => "4",
            5 => "5",
            6 => "6",
            7 => "7",
            8 => "8",
            9 => "9",
            10 => "T",
            11 => "J",
            12 => "Q",
            13 => "K",
            14 => "A",
            _ => "?",
        }
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank_label(), self.suit.symbol())
    }
}
