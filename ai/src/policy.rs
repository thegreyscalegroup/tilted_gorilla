//! Bot decision policy.
//!
//! The policy converts an equity estimate plus the pot odds into a concrete
//! [`Action`]. Three tiers tune tightness, aggression, and bluff frequency —
//! the difficulty setting PokerTH never had. The bots fold when the math says
//! fold, value-raise strong hands, call correct prices, and bluff at a
//! controlled rate, so they neither shove blindly nor collapse to aggression.

use tg_engine::hand::{Action, Hand};
use tg_engine::rng::Rng;

use crate::equity::equity;

/// Difficulty tier. Higher tiers think harder (more Monte-Carlo samples), demand
/// less overlay to continue, and bluff more.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Tight-passive. Plays strong hands, almost never bluffs. Beatable.
    Rock,
    /// Solid tight-aggressive. Respects pot odds, bets for value, mixes in a
    /// few bluffs.
    Grinder,
    /// Aggressive and tricky. Thin value, frequent semi-bluffs and bluff-raises,
    /// pressures marginal spots.
    Shark,
}

struct Params {
    iters: usize,
    /// Equity at/above which we raise for value.
    value_threshold: f64,
    /// Multiplier on the pot-odds break-even before we'll call. <1 calls looser.
    call_cushion: f64,
    /// Chance to fire a bluff/semi-bluff when we otherwise wouldn't.
    bluff_freq: f64,
    /// Raise size as a fraction of the current pot.
    raise_frac: f64,
}

impl Tier {
    fn params(self) -> Params {
        match self {
            Tier::Rock => Params {
                iters: 250,
                value_threshold: 0.82,
                call_cushion: 1.25,
                bluff_freq: 0.02,
                raise_frac: 0.55,
            },
            Tier::Grinder => Params {
                iters: 450,
                value_threshold: 0.68,
                call_cushion: 1.0,
                bluff_freq: 0.10,
                raise_frac: 0.68,
            },
            Tier::Shark => Params {
                iters: 700,
                value_threshold: 0.60,
                call_cushion: 0.85,
                bluff_freq: 0.24,
                raise_frac: 0.85,
            },
        }
    }
}

/// Decide an action for `seat` in the current hand state. The seat must be the
/// one on turn. Never returns an illegal action.
pub fn decide(hand: &Hand, seat: usize, tier: Tier, rng: &mut Rng) -> Action {
    let Some(legal) = hand.legal_actions() else {
        return Action::Fold;
    };
    let p = tier.params();

    let hole = match hand.seats[seat].hole {
        Some(h) => h,
        None => return Action::Fold, // shouldn't happen for a live seat
    };

    let opponents = hand
        .seats
        .iter()
        .enumerate()
        .filter(|(i, s)| *i != seat && s.in_hand())
        .count();

    let eq = equity(hole, &hand.board, opponents, p.iters, rng);

    let pot = hand.pot_total().max(1);
    let call_cost = legal.call_cost;
    let pot_after = pot + call_cost;
    // Break-even equity needed to call: price / (pot + price).
    let required = if pot_after > 0 {
        call_cost as f64 / pot_after as f64
    } else {
        0.0
    };

    // Helper to size and clamp a raise to a legal total.
    let raise_to = |frac: f64| -> u32 {
        let extra = (pot as f64 * frac).round() as u32;
        (hand.current_bet + extra).clamp(legal.min_raise_to, legal.max_raise_to)
    };

    if legal.can_check {
        // No bet to face. Value-bet strong hands; occasionally bluff; else check.
        if eq >= p.value_threshold && legal.can_raise {
            return Action::Raise { to: raise_to(p.raise_frac) };
        }
        // Semi-bluff/bluff with hands that have some but not showdown-winning
        // equity, at the tier's frequency.
        if legal.can_raise && eq < p.value_threshold && rng_bool(rng, p.bluff_freq) {
            return Action::Raise { to: raise_to(p.raise_frac * 0.8) };
        }
        return Action::Check;
    }

    // Facing a bet.
    // Big hands raise for value (bigger with more equity).
    if eq >= p.value_threshold && legal.can_raise {
        let frac = if eq > 0.9 { p.raise_frac * 1.3 } else { p.raise_frac };
        return Action::Raise { to: raise_to(frac) };
    }

    // Call when equity beats the (cushioned) price.
    if eq >= required * p.call_cushion {
        return Action::Call;
    }

    // Otherwise mostly fold, but bluff-raise occasionally when we have a little
    // equity to fall back on (a semi-bluff).
    if legal.can_raise && eq > 0.30 && rng_bool(rng, p.bluff_freq) {
        return Action::Raise { to: raise_to(p.raise_frac) };
    }

    // If it's free to stay (call_cost == 0 but can't check — shouldn't happen)
    // prefer calling over folding.
    if call_cost == 0 {
        return Action::Call;
    }

    Action::Fold
}

/// True with probability `p` (0.0..=1.0).
fn rng_bool(rng: &mut Rng, p: f64) -> bool {
    if p <= 0.0 {
        return false;
    }
    if p >= 1.0 {
        return true;
    }
    // 24 bits of resolution is plenty for bluff frequencies.
    let r = (rng.next_u64() >> 40) as f64 / (1u64 << 24) as f64;
    r < p
}
