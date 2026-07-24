//! Human-readable descriptions of hands ("Pair of Kings", "Ace-high flush",
//! "Ace-King suited"). Pure formatting over a [`HandValue`] or two hole cards —
//! the UI uses these for the hand-strength label.

use crate::card::{Card, Suit};
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

/// Detect drawing hands (flush / straight draws) from the hole cards plus the
/// current board. Returns short labels like "flush draw", "open-ended straight
/// draw", "gutshot straight draw". Only meaningful on the flop and turn (5–6
/// cards total); returns nothing pre-flop or once the board is complete, and
/// nothing when the draw is already a made hand.
pub fn draws(hole: [Card; 2], board: &[Card]) -> Vec<&'static str> {
    let mut cards: Vec<Card> = board.to_vec();
    cards.push(hole[0]);
    cards.push(hole[1]);
    // Only flop (5) and turn (6) have live draws worth calling out.
    if cards.len() < 5 || cards.len() > 6 {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Flush draw: exactly four of one suit (five+ is a made flush, not a draw).
    let mut suit_counts = [0u8; 4];
    for c in &cards {
        let idx = match c.suit {
            Suit::Clubs => 0,
            Suit::Diamonds => 1,
            Suit::Hearts => 2,
            Suit::Spades => 3,
        };
        suit_counts[idx] += 1;
    }
    if suit_counts.iter().any(|&n| n == 4) {
        out.push("flush draw");
    }

    // Straight draw: over every 5-rank window, count distinct ranks held. Ace
    // plays high and low. If we hold exactly 4 of a window, the fifth completes
    // a straight; a made straight (all 5) suppresses the draw.
    let mut present = [false; 15]; // ranks 1..=14, with 1 = ace-low
    for c in &cards {
        present[c.rank as usize] = true;
    }
    if present[14] {
        present[1] = true;
    }
    let mut made = false;
    let mut completing = std::collections::BTreeSet::new();
    for low in 1..=10usize {
        let window = [low, low + 1, low + 2, low + 3, low + 4];
        let held = window.iter().filter(|&&r| present[r]).count();
        if held == 5 {
            made = true;
        } else if held == 4 {
            if let Some(&miss) = window.iter().find(|&&r| !present[r]) {
                completing.insert(if miss == 1 { 14 } else { miss });
            }
        }
    }
    if !made {
        match completing.len() {
            0 => {}
            1 => out.push("gutshot straight draw"),
            _ => out.push("open-ended straight draw"),
        }
    }

    out
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
