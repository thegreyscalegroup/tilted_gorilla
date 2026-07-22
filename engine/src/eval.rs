//! Five- and seven-card hand evaluation.
//!
//! [`eval7`] finds the best 5-card poker hand out of 7 cards by checking all 21
//! combinations and taking the maximum. This is simple and provably correct;
//! it's fast enough for real-time play and Monte-Carlo equity (~a few hundred
//! ns per hand). We can swap in a lookup-table evaluator later without changing
//! the public API.

use crate::card::Card;

/// Poker hand categories, ordered weakest → strongest. The derived `Ord` makes
/// `StraightFlush > FourOfAKind > ... > HighCard`, exactly as ranked at showdown.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Category {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

impl Category {
    /// All categories, weakest → strongest.
    pub const ALL: [Category; 9] = [
        Category::HighCard,
        Category::OnePair,
        Category::TwoPair,
        Category::ThreeOfAKind,
        Category::Straight,
        Category::Flush,
        Category::FullHouse,
        Category::FourOfAKind,
        Category::StraightFlush,
    ];

    /// Stable 0..9 index (matches `ALL` ordering) for tallying distributions.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Short display name for an odds table.
    pub fn label(self) -> &'static str {
        match self {
            Category::HighCard => "High card",
            Category::OnePair => "Pair",
            Category::TwoPair => "Two pair",
            Category::ThreeOfAKind => "Trips",
            Category::Straight => "Straight",
            Category::Flush => "Flush",
            Category::FullHouse => "Full house",
            Category::FourOfAKind => "Quads",
            Category::StraightFlush => "Str. flush",
        }
    }
}

/// A fully comparable hand strength.
///
/// Field order matters: `category` is compared first, then `tiebreak` — a list
/// of ranks in decreasing significance (pair rank, then kickers, etc.), padded
/// with zeros. Two `HandValue`s compare exactly as two poker hands do.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct HandValue {
    pub category: Category,
    pub tiebreak: [u8; 5],
}

/// Evaluate exactly five cards.
pub fn eval5(cards: &[Card; 5]) -> HandValue {
    // Ranks, sorted descending — the basis for every tiebreak.
    let mut ranks = [
        cards[0].rank,
        cards[1].rank,
        cards[2].rank,
        cards[3].rank,
        cards[4].rank,
    ];
    ranks.sort_unstable_by(|a, b| b.cmp(a));

    let is_flush = cards.iter().all(|c| c.suit == cards[0].suit);
    let straight_high = straight_high_card(&ranks);

    // Group ranks by how many times each appears, sorted by (count desc, rank
    // desc). This single structure drives pair/trips/quads/full-house logic.
    let groups = rank_groups(&ranks);

    // Straight flush (incl. royal, which is just an ace-high straight flush).
    if is_flush {
        if let Some(high) = straight_high {
            return HandValue {
                category: Category::StraightFlush,
                tiebreak: [high, 0, 0, 0, 0],
            };
        }
    }

    match groups[0].1 {
        4 => {
            let quad = groups[0].0;
            let kicker = groups[1].0;
            HandValue {
                category: Category::FourOfAKind,
                tiebreak: [quad, kicker, 0, 0, 0],
            }
        }
        3 if groups[1].1 >= 2 => {
            let trips = groups[0].0;
            let pair = groups[1].0;
            HandValue {
                category: Category::FullHouse,
                tiebreak: [trips, pair, 0, 0, 0],
            }
        }
        _ if is_flush => HandValue {
            category: Category::Flush,
            tiebreak: ranks,
        },
        _ if straight_high.is_some() => HandValue {
            category: Category::Straight,
            tiebreak: [straight_high.unwrap(), 0, 0, 0, 0],
        },
        3 => {
            let trips = groups[0].0;
            let k1 = groups[1].0;
            let k2 = groups[2].0;
            HandValue {
                category: Category::ThreeOfAKind,
                tiebreak: [trips, k1, k2, 0, 0],
            }
        }
        2 if groups[1].1 == 2 => {
            let hi_pair = groups[0].0;
            let lo_pair = groups[1].0;
            let kicker = groups[2].0;
            HandValue {
                category: Category::TwoPair,
                tiebreak: [hi_pair, lo_pair, kicker, 0, 0],
            }
        }
        2 => {
            let pair = groups[0].0;
            let k1 = groups[1].0;
            let k2 = groups[2].0;
            let k3 = groups[3].0;
            HandValue {
                category: Category::OnePair,
                tiebreak: [pair, k1, k2, k3, 0],
            }
        }
        _ => HandValue {
            category: Category::HighCard,
            tiebreak: ranks,
        },
    }
}

/// Evaluate the best 5-card hand from 5, 6, or 7 cards.
pub fn eval7(cards: &[Card]) -> HandValue {
    assert!(
        (5..=7).contains(&cards.len()),
        "eval7 expects 5–7 cards, got {}",
        cards.len()
    );
    let n = cards.len();
    let mut best: Option<HandValue> = None;
    // Iterate every 5-card subset via nested index selection.
    for a in 0..n {
        for b in (a + 1)..n {
            for c in (b + 1)..n {
                for d in (c + 1)..n {
                    for e in (d + 1)..n {
                        let hand = [cards[a], cards[b], cards[c], cards[d], cards[e]];
                        let v = eval5(&hand);
                        if best.map_or(true, |cur| v > cur) {
                            best = Some(v);
                        }
                    }
                }
            }
        }
    }
    best.expect("at least one 5-card combination exists")
}

/// If the five (descending) ranks form a straight, return its high card.
/// Handles the wheel (A-2-3-4-5), which is a 5-high straight.
fn straight_high_card(desc_ranks: &[u8; 5]) -> Option<u8> {
    // Need five distinct ranks.
    for w in desc_ranks.windows(2) {
        if w[0] == w[1] {
            return None;
        }
    }
    // Normal run: each step down by exactly one.
    let normal = desc_ranks
        .windows(2)
        .all(|w| w[0] == w[1] + 1);
    if normal {
        return Some(desc_ranks[0]);
    }
    // Wheel: A,5,4,3,2 → treat as 5-high.
    if *desc_ranks == [14, 5, 4, 3, 2] {
        return Some(5);
    }
    None
}

/// Ranks grouped as (rank, count), sorted by count desc then rank desc.
/// Padded to five entries with (0, 0) so callers can index freely.
fn rank_groups(desc_ranks: &[u8; 5]) -> [(u8, u8); 5] {
    let mut counts: Vec<(u8, u8)> = Vec::with_capacity(5);
    for &r in desc_ranks {
        if let Some(entry) = counts.iter_mut().find(|(rank, _)| *rank == r) {
            entry.1 += 1;
        } else {
            counts.push((r, 1));
        }
    }
    counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    let mut out = [(0u8, 0u8); 5];
    for (i, g) in counts.into_iter().enumerate() {
        out[i] = g;
    }
    out
}
