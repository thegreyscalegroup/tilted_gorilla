//! The Texas Hold'em hand state machine.
//!
//! A [`Hand`] runs one deal from posting blinds through showdown. Callers drive
//! it by reading [`Hand::to_act`] / [`Hand::legal_actions`] and applying
//! [`Hand::apply`]. All money movement, street progression, and pot awarding
//! lives here; the UI and AI only observe state and submit actions.

use crate::card::Card;
use crate::deck::Deck;
use crate::eval::eval7;
use crate::pot::{build_pots, Pot};
use crate::rng::Rng;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
    Showdown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeatStatus {
    /// In the hand and able to act.
    Active,
    /// Folded this hand.
    Folded,
    /// All chips committed; stays in for showdown but cannot act.
    AllIn,
    /// Not dealt in (busted or sitting out).
    SittingOut,
}

#[derive(Clone, Debug)]
pub struct Seat {
    pub stack: u32,
    /// Chips committed on the current street.
    pub street_bet: u32,
    /// Chips committed across the whole hand (drives side pots).
    pub committed: u32,
    pub hole: Option<[Card; 2]>,
    pub status: SeatStatus,
    /// Whether this seat has acted since the last aggressive action. Used to
    /// decide when a betting round is complete.
    acted: bool,
}

impl Seat {
    pub fn new(stack: u32) -> Seat {
        Seat {
            stack,
            street_bet: 0,
            committed: 0,
            hole: None,
            status: SeatStatus::SittingOut,
            acted: false,
        }
    }

    pub fn in_hand(&self) -> bool {
        matches!(self.status, SeatStatus::Active | SeatStatus::AllIn)
    }
}

/// A player action. `Raise { to }` is the *total* street commitment being
/// raised to (an opening bet is a raise from zero). `Call` and `Check` take no
/// amount; the engine computes what's owed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Fold,
    Check,
    Call,
    Raise { to: u32 },
}

/// What a seat is currently allowed to do, with computed amounts to make UI and
/// AI code straightforward.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Legal {
    pub can_fold: bool,
    pub can_check: bool,
    /// Chips needed to call (0 if checking is free). Capped at the seat's stack.
    pub call_cost: u32,
    pub can_raise: bool,
    /// Smallest legal total to raise `to` (a full min-raise), when `can_raise`.
    pub min_raise_to: u32,
    /// Largest total (all-in), when `can_raise`.
    pub max_raise_to: u32,
}

/// Result of a showdown: what each seat won (0 if nothing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payouts {
    pub winnings: Vec<u32>,
    pub pots: Vec<Pot>,
}

pub struct Hand {
    pub seats: Vec<Seat>,
    pub button: usize,
    pub sb: u32,
    pub bb: u32,
    pub board: Vec<Card>,
    pub street: Street,
    pub to_act: Option<usize>,
    /// Highest `street_bet` anyone has posted this street.
    pub current_bet: u32,
    /// Minimum raise increment allowed right now.
    pub min_raise: u32,
    deck: Deck,
}

impl Hand {
    /// Start a hand. `stacks[i]` seats a player with that many chips (0 = empty
    /// seat / sitting out). `button` is the dealer seat index. Blinds are posted
    /// and hole cards dealt; `to_act` points at the first player to act.
    pub fn start(stacks: &[u32], button: usize, sb: u32, bb: u32, rng: &mut Rng) -> Hand {
        let mut seats: Vec<Seat> = stacks.iter().map(|&s| Seat::new(s)).collect();
        for seat in seats.iter_mut() {
            if seat.stack > 0 {
                seat.status = SeatStatus::Active;
            }
        }

        let mut deck = Deck::standard();
        deck.shuffle(rng);

        let mut hand = Hand {
            seats,
            button,
            sb,
            bb,
            board: Vec::with_capacity(5),
            street: Street::Preflop,
            to_act: None,
            current_bet: 0,
            min_raise: bb,
            deck,
        };

        hand.deal_hole_cards();
        hand.post_blinds();
        hand.to_act = Some(hand.first_to_act_preflop());
        hand
    }

    fn active_seats(&self) -> Vec<usize> {
        (0..self.seats.len())
            .filter(|&i| self.seats[i].status == SeatStatus::Active)
            .collect()
    }

    fn seats_in_hand(&self) -> Vec<usize> {
        (0..self.seats.len())
            .filter(|&i| self.seats[i].in_hand())
            .collect()
    }

    fn deal_hole_cards(&mut self) {
        let order: Vec<usize> = (0..self.seats.len())
            .filter(|&i| self.seats[i].status == SeatStatus::Active)
            .collect();
        for &i in &order {
            let a = self.deck.deal().expect("deck exhausted dealing hole cards");
            let b = self.deck.deal().expect("deck exhausted dealing hole cards");
            self.seats[i].hole = Some([a, b]);
        }
    }

    /// Move a bet from a seat's stack into the pot, marking all-in if it empties
    /// the stack. Returns the amount actually taken (may be less than requested
    /// when short-stacked).
    fn commit(&mut self, seat: usize, amount: u32) -> u32 {
        let paid = amount.min(self.seats[seat].stack);
        self.seats[seat].stack -= paid;
        self.seats[seat].street_bet += paid;
        self.seats[seat].committed += paid;
        if self.seats[seat].stack == 0 && self.seats[seat].in_hand() {
            self.seats[seat].status = SeatStatus::AllIn;
        }
        paid
    }

    fn post_blinds(&mut self) {
        let actives = self.active_seats();
        let n = actives.len();
        debug_assert!(n >= 2, "need at least two players");
        // Heads-up: the button posts the small blind. Otherwise SB is left of
        // the button, BB one further.
        let (sb_seat, bb_seat) = if n == 2 {
            (self.button, self.next_active(self.button))
        } else {
            let sb = self.next_active(self.button);
            (sb, self.next_active(sb))
        };
        self.commit(sb_seat, self.sb);
        self.commit(bb_seat, self.bb);
        self.current_bet = self.bb;
        self.min_raise = self.bb;
    }

    fn first_to_act_preflop(&self) -> usize {
        let actives = self.active_seats();
        if actives.len() == 2 {
            // Heads-up: SB (the button) acts first preflop.
            self.button
        } else {
            // Under the gun: left of the big blind.
            let sb = self.next_active(self.button);
            let bb = self.next_active(sb);
            self.next_active(bb)
        }
    }

    /// Next seat (cyclically) that is still Active (can act).
    fn next_active(&self, from: usize) -> usize {
        let n = self.seats.len();
        let mut i = (from + 1) % n;
        while self.seats[i].status != SeatStatus::Active {
            i = (i + 1) % n;
            if i == from {
                break;
            }
        }
        i
    }

    /// What the seat on turn may legally do.
    pub fn legal_actions(&self) -> Option<Legal> {
        let seat_idx = self.to_act?;
        let seat = &self.seats[seat_idx];
        let owed = self.current_bet.saturating_sub(seat.street_bet);
        let call_cost = owed.min(seat.stack);
        let can_check = owed == 0;
        // You can raise only if you have chips beyond a call.
        let can_raise = seat.stack > owed;
        let min_raise_to = self.current_bet + self.min_raise;
        let max_raise_to = seat.street_bet + seat.stack; // all-in total
        Some(Legal {
            can_fold: true,
            can_check,
            call_cost,
            can_raise,
            // Clamp the min to the all-in cap for short stacks.
            min_raise_to: min_raise_to.min(max_raise_to),
            max_raise_to,
        })
    }

    /// Apply an action for the seat on turn. Returns `Err` with a reason if the
    /// action is illegal; state is unchanged in that case.
    pub fn apply(&mut self, action: Action) -> Result<(), &'static str> {
        let seat_idx = self.to_act.ok_or("no seat to act")?;
        let legal = self.legal_actions().ok_or("no seat to act")?;

        match action {
            Action::Fold => {
                self.seats[seat_idx].status = SeatStatus::Folded;
            }
            Action::Check => {
                if !legal.can_check {
                    return Err("cannot check facing a bet");
                }
                self.seats[seat_idx].acted = true;
            }
            Action::Call => {
                self.commit(seat_idx, legal.call_cost);
                self.seats[seat_idx].acted = true;
            }
            Action::Raise { to } => {
                if !legal.can_raise {
                    return Err("cannot raise");
                }
                // Allow an all-in that falls short of a full min-raise, but
                // otherwise enforce the minimum.
                let is_all_in = to == legal.max_raise_to;
                if to < legal.min_raise_to && !is_all_in {
                    return Err("raise below minimum");
                }
                if to > legal.max_raise_to {
                    return Err("raise exceeds stack");
                }
                let raise_increment = to.saturating_sub(self.current_bet);
                let added = to - self.seats[seat_idx].street_bet;
                self.commit(seat_idx, added);
                // A full raise reopens the action and sets the new min; a short
                // all-in raises the bet level but does not reset others' option.
                if raise_increment >= self.min_raise {
                    self.min_raise = raise_increment;
                    self.reopen_action(seat_idx);
                }
                self.current_bet = self.current_bet.max(self.seats[seat_idx].street_bet);
                self.seats[seat_idx].acted = true;
            }
        }

        self.advance();
        Ok(())
    }

    /// After a full raise, everyone else who's still active must act again.
    fn reopen_action(&mut self, raiser: usize) {
        for (i, seat) in self.seats.iter_mut().enumerate() {
            if i != raiser && seat.status == SeatStatus::Active {
                seat.acted = false;
            }
        }
    }

    /// Move the turn forward, closing the street or ending the hand as needed.
    fn advance(&mut self) {
        // Hand ends immediately if only one player remains in the hand.
        if self.seats_in_hand().len() <= 1 {
            self.to_act = None;
            self.street = Street::Showdown;
            return;
        }

        if self.street_complete() {
            self.close_street();
            return;
        }

        // Find the next active seat that still needs to act.
        let from = self.to_act.expect("advancing with no actor");
        let mut i = self.next_active(from);
        // Skip anyone who has already acted and matched the bet.
        let start = i;
        loop {
            let s = &self.seats[i];
            let matched = s.street_bet == self.current_bet;
            if s.status == SeatStatus::Active && !(s.acted && matched) {
                self.to_act = Some(i);
                return;
            }
            i = self.next_active(i);
            if i == start {
                break;
            }
        }
        // Nobody left to act → close the street.
        self.close_street();
    }

    /// A betting round is done when every still-active seat has acted and has
    /// matched the current bet (all-in players are exempt — they can't act).
    fn street_complete(&self) -> bool {
        self.active_seats().iter().all(|&i| {
            let s = &self.seats[i];
            s.acted && s.street_bet == self.current_bet
        })
    }

    fn close_street(&mut self) {
        // Reset per-street betting state.
        for seat in self.seats.iter_mut() {
            seat.street_bet = 0;
            seat.acted = false;
        }
        self.current_bet = 0;
        self.min_raise = self.bb;

        match self.street {
            Street::Preflop => {
                self.deal_board(3);
                self.street = Street::Flop;
            }
            Street::Flop => {
                self.deal_board(1);
                self.street = Street::Turn;
            }
            Street::Turn => {
                self.deal_board(1);
                self.street = Street::River;
            }
            Street::River => {
                self.street = Street::Showdown;
                self.to_act = None;
                return;
            }
            Street::Showdown => {
                self.to_act = None;
                return;
            }
        }

        // If one or fewer players can still act (rest all-in), run out the board
        // to showdown without further betting.
        if self.active_seats().len() <= 1 {
            self.run_out_board();
            self.street = Street::Showdown;
            self.to_act = None;
        } else {
            self.to_act = Some(self.first_to_act_postflop());
        }
    }

    fn first_to_act_postflop(&self) -> usize {
        // First active seat left of the button.
        self.next_active(self.button)
    }

    fn deal_board(&mut self, count: usize) {
        // Burn one card, per convention, then deal `count`.
        self.deck.deal();
        for _ in 0..count {
            if let Some(c) = self.deck.deal() {
                self.board.push(c);
            }
        }
    }

    /// Deal any remaining community cards (used when betting is closed because
    /// everyone is all-in).
    fn run_out_board(&mut self) {
        while self.board.len() < 5 {
            let need = if self.board.is_empty() { 3 } else { 1 };
            self.deal_board(need);
        }
    }

    /// Whether the hand has reached showdown / is over.
    pub fn is_over(&self) -> bool {
        self.street == Street::Showdown
    }

    /// Settle the hand: award pots to the best hands and pay winners. Idempotent
    /// once called — returns who won what. Only valid at showdown.
    pub fn settle(&mut self) -> Payouts {
        debug_assert!(self.is_over(), "settle called before showdown");
        let n = self.seats.len();
        let contributions: Vec<u32> = self.seats.iter().map(|s| s.committed).collect();
        let folded: Vec<bool> = self
            .seats
            .iter()
            .map(|s| !s.in_hand())
            .collect();
        let pots = build_pots(&contributions, &folded);

        let mut winnings = vec![0u32; n];
        for pot in &pots {
            // Uncontested pot (everyone else folded): award directly. This also
            // covers hands that end before the board is complete, where there's
            // nothing to evaluate.
            if pot.eligible.len() == 1 {
                winnings[pot.eligible[0]] += pot.amount;
                continue;
            }

            // Rank each eligible seat's best 7-card hand.
            let best = pot
                .eligible
                .iter()
                .filter_map(|&i| {
                    let hole = self.seats[i].hole?;
                    let mut seven = self.board.clone();
                    seven.push(hole[0]);
                    seven.push(hole[1]);
                    Some((i, eval7(&seven)))
                })
                .fold(None::<crate::eval::HandValue>, |acc, (_, v)| match acc {
                    Some(cur) if cur >= v => Some(cur),
                    _ => Some(v),
                });
            let Some(best) = best else { continue };

            let winners: Vec<usize> = pot
                .eligible
                .iter()
                .copied()
                .filter(|&i| {
                    self.seats[i].hole.map_or(false, |hole| {
                        let mut seven = self.board.clone();
                        seven.push(hole[0]);
                        seven.push(hole[1]);
                        eval7(&seven) == best
                    })
                })
                .collect();

            // Split as evenly as possible; odd chips go to the earliest seats
            // left of the button (a common house rule; simplified here to seat
            // order).
            let share = pot.amount / winners.len() as u32;
            let mut remainder = pot.amount % winners.len() as u32;
            for &w in &winners {
                let mut amt = share;
                if remainder > 0 {
                    amt += 1;
                    remainder -= 1;
                }
                winnings[w] += amt;
            }
        }

        for (i, w) in winnings.iter().enumerate() {
            self.seats[i].stack += *w;
        }

        Payouts { winnings, pots }
    }

    /// Total chips currently in all pots (committed this hand, not yet awarded).
    pub fn pot_total(&self) -> u32 {
        self.seats.iter().map(|s| s.committed).sum()
    }
}
