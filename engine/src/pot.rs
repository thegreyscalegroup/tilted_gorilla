//! Side-pot construction.
//!
//! When players go all-in for different amounts, the chips split into a main
//! pot plus one or more side pots, each with its own set of eligible winners.
//! This module turns per-player contributions into those pots. It's kept
//! separate from the betting flow because the layering math is fiddly and
//! deserves its own tests.

/// One pot layer: an amount and the seats eligible to win it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pot {
    pub amount: u32,
    /// Seat indices still in the hand (not folded) who can win this pot.
    pub eligible: Vec<usize>,
}

/// Build pots from each seat's total contribution over the hand and whether
/// that seat folded. Folded players' chips still form part of the pots, but
/// they are never eligible to win.
///
/// The classic algorithm: walk the distinct contribution levels from low to
/// high; each level adds a layer funded by everyone who reached it, winnable by
/// the non-folded subset of them.
pub fn build_pots(contributions: &[u32], folded: &[bool]) -> Vec<Pot> {
    assert_eq!(contributions.len(), folded.len());

    let mut levels: Vec<u32> = contributions.iter().copied().filter(|&c| c > 0).collect();
    levels.sort_unstable();
    levels.dedup();

    let mut pots: Vec<Pot> = Vec::new();
    let mut prev = 0u32;
    for level in levels {
        let layer = level - prev;
        let mut amount = 0u32;
        let mut eligible = Vec::new();
        for (seat, &contrib) in contributions.iter().enumerate() {
            if contrib >= level {
                amount += layer;
                if !folded[seat] {
                    eligible.push(seat);
                }
            }
        }
        if amount > 0 {
            // Merge into the previous pot if the eligible set is identical —
            // avoids emitting redundant single-eligibility layers.
            if let Some(last) = pots.last_mut() {
                if last.eligible == eligible {
                    last.amount += amount;
                    prev = level;
                    continue;
                }
            }
            pots.push(Pot { amount, eligible });
        }
        prev = level;
    }
    pots
}
