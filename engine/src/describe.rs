//! Human-readable descriptions of hands ("Pair of Kings", "Ace-high flush",
//! "Ace-King suited"). Pure formatting over a [`HandValue`] or two hole cards —
//! the UI uses these for the hand-strength label.

use crate::card::Card;
use crate::eval::{Category, HandValue};

/// Singular rank name, e.g. 14 → "Ace", 10 → "Ten".
fn rank_singular(r: u8) -> &'static str {
    match r {
        2 => "Two",
        3 => "Three",
        4 => "Four",
        5 => "Five",
        6 => "Six",
        7 => "Seven",
        8 => "Eight",
        9 => "Nine",
        10 => "Ten",
        11 => "Jack",
        12 => "Queen",
        13 => "King",
        14 => "Ace",
        _ => "?",
    }
}

/// Plural rank name, e.g. 14 → "Aces", 6 → "Sixes".
fn rank_plural(r: u8) -> &'static str {
    match r {
        2 => "Twos",
        3 => "Threes",
        4 => "Fours",
        5 => "Fives",
        6 => "Sixes",
        7 => "Sevens",
        8 => "Eights",
        9 => "Nines",
        10 => "Tens",
        11 => "Jacks",
        12 => "Queens",
        13 => "Kings",
        14 => "Aces",
        _ => "?",
    }
}

/// Describe a made (5+ card) hand value in natural language.
pub fn describe(v: &HandValue) -> String {
    let t = v.tiebreak;
    match v.category {
        Category::HighCard => format!("{} high", rank_singular(t[0])),
        Category::OnePair => format!("Pair of {}", rank_plural(t[0])),
        Category::TwoPair => format!(
            "Two pair, {} and {}",
            rank_plural(t[0]),
            rank_plural(t[1])
        ),
        Category::ThreeOfAKind => format!("Three of a kind, {}", rank_plural(t[0])),
        Category::Straight => format!("{}-high straight", rank_singular(t[0])),
        Category::Flush => format!("Flush, {} high", rank_singular(t[0])),
        Category::FullHouse => format!(
            "Full house, {} full of {}",
            rank_plural(t[0]),
            rank_plural(t[1])
        ),
        Category::FourOfAKind => format!("Four of a kind, {}", rank_plural(t[0])),
        Category::StraightFlush => {
            if t[0] == 14 {
                "Royal flush".to_string()
            } else {
                format!("{}-high straight flush", rank_singular(t[0]))
            }
        }
    }
}

/// Describe two hole cards the way players name starting hands, e.g.
/// "Pair of Kings", "Ace-King suited", "Queen-Seven offsuit".
pub fn describe_hole(hole: [Card; 2]) -> String {
    if hole[0].rank == hole[1].rank {
        return format!("Pair of {}", rank_plural(hole[0].rank));
    }
    let (hi, lo) = if hole[0].rank >= hole[1].rank {
        (hole[0], hole[1])
    } else {
        (hole[1], hole[0])
    };
    let suited = if hi.suit == lo.suit { "suited" } else { "offsuit" };
    format!("{}-{} {}", rank_singular(hi.rank), rank_singular(lo.rank), suited)
}
