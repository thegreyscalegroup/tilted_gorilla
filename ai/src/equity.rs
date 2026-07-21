//! Monte-Carlo hand equity.
//!
//! Given the hero's two hole cards, the current board, and how many opponents
//! are still live, we estimate the probability the hero wins (with ties counted
//! fractionally) by repeatedly dealing random opponents and random board
//! run-outs from the remaining deck. This is the number the policy layer turns
//! into decisions — and it's why the AI actually understands hand strength,
//! unlike PokerTH's shove-or-fold bot.

use tg_engine::card::{Card, Suit, MAX_RANK, MIN_RANK};
use tg_engine::eval::eval7;
use tg_engine::rng::Rng;

/// Estimate hero equity over `iters` random simulations. Returns a value in
/// `0.0..=1.0`: 1.0 means the hero always wins, 0.5 a coin flip.
///
/// `opponents` is the number of other players still in the hand. Their hole
/// cards are unknown, so we deal them at random — exactly the information a real
/// player has.
pub fn equity(hero: [Card; 2], board: &[Card], opponents: usize, iters: usize, rng: &mut Rng) -> f64 {
    if opponents == 0 {
        return 1.0;
    }

    // The pool of cards nobody has seen: full deck minus hero's cards and the
    // visible board.
    let mut known = [false; 52];
    mark(&mut known, hero[0]);
    mark(&mut known, hero[1]);
    for &c in board {
        mark(&mut known, c);
    }
    let pool: Vec<Card> = all_cards().into_iter().filter(|c| !known[index(*c)]).collect();

    let need_board = 5 - board.len();
    let draw_count = opponents * 2 + need_board;

    let mut score = 0.0f64;
    let mut scratch = pool.clone();
    for _ in 0..iters {
        // Partial Fisher–Yates: sample `draw_count` distinct cards to the front.
        for i in 0..draw_count {
            let j = i + rng.below((scratch.len() - i) as u32) as usize;
            scratch.swap(i, j);
        }
        let sample = &scratch[..draw_count];

        // Complete the board.
        let mut full_board = board.to_vec();
        full_board.extend_from_slice(&sample[opponents * 2..]);

        // Hero's best hand.
        let mut hero_seven = full_board.clone();
        hero_seven.push(hero[0]);
        hero_seven.push(hero[1]);
        let hero_val = eval7(&hero_seven);

        // Compare against each opponent; track how many share the top value.
        let mut hero_is_best = true;
        let mut ties = 1; // hero
        for o in 0..opponents {
            let mut opp_seven = full_board.clone();
            opp_seven.push(sample[o * 2]);
            opp_seven.push(sample[o * 2 + 1]);
            let opp_val = eval7(&opp_seven);
            if opp_val > hero_val {
                hero_is_best = false;
                break;
            } else if opp_val == hero_val {
                ties += 1;
            }
        }

        if hero_is_best {
            score += 1.0 / ties as f64;
        }
    }

    score / iters as f64
}

fn index(c: Card) -> usize {
    let suit = match c.suit {
        Suit::Clubs => 0,
        Suit::Diamonds => 1,
        Suit::Hearts => 2,
        Suit::Spades => 3,
    };
    suit * 13 + (c.rank - MIN_RANK) as usize
}

fn mark(known: &mut [bool; 52], c: Card) {
    known[index(c)] = true;
}

fn all_cards() -> Vec<Card> {
    let mut v = Vec::with_capacity(52);
    for suit in Suit::ALL {
        for rank in MIN_RANK..=MAX_RANK {
            v.push(Card::new(rank, suit));
        }
    }
    v
}
