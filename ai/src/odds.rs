//! Outcome odds — the probability the hero's hand *finishes* as each category
//! by the river, given the current board. This is PokerTH's signature readout:
//! it depends only on the remaining community cards (not opponents), so we
//! Monte-Carlo random board run-outs and tally the resulting hand category.

use tg_engine::card::Card;
use tg_engine::eval::eval7;
use tg_engine::rng::Rng;

use crate::equity::{all_cards, index, mark};

/// Probability distribution over the nine hand categories (indexed by
/// [`tg_engine::eval::Category::index`]) for the hero's final hand.
///
/// If the board is already complete, the result is a certainty (1.0 on the
/// current category). Otherwise we sample the missing board cards `iters` times.
pub fn outcome_distribution(hole: [Card; 2], board: &[Card], iters: usize, rng: &mut Rng) -> [f64; 9] {
    let need = 5 - board.len();

    // Board complete: the outcome is already decided.
    if need == 0 {
        let mut seven = board.to_vec();
        seven.push(hole[0]);
        seven.push(hole[1]);
        let mut d = [0.0; 9];
        d[eval7(&seven).category.index()] = 1.0;
        return d;
    }

    // Cards nobody has seen: full deck minus hole and board.
    let mut known = [false; 52];
    mark(&mut known, hole[0]);
    mark(&mut known, hole[1]);
    for &c in board {
        mark(&mut known, c);
    }
    let pool: Vec<Card> = all_cards().into_iter().filter(|c| !known[index(*c)]).collect();

    let mut counts = [0u64; 9];
    let mut scratch = pool;
    for _ in 0..iters {
        // Sample `need` distinct board cards to the front (partial shuffle).
        for i in 0..need {
            let j = i + rng.below((scratch.len() - i) as u32) as usize;
            scratch.swap(i, j);
        }
        let mut seven = board.to_vec();
        seven.extend_from_slice(&scratch[..need]);
        seven.push(hole[0]);
        seven.push(hole[1]);
        counts[eval7(&seven).category.index()] += 1;
    }

    let total = iters as f64;
    let mut d = [0.0; 9];
    for (i, &c) in counts.iter().enumerate() {
        d[i] = c as f64 / total;
    }
    d
}
