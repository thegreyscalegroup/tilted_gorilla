//! UI-facing game controller.
//!
//! Wraps the rules [`Hand`] and the [`tg_ai`] bots into a single object the
//! Leptos view drives. Bots act **one step at a time** ([`Game::step_bot`]) so
//! the UI can pace them with a timer and play a sound per action, instead of the
//! whole table resolving in a single instant. It also computes the human's live
//! equity — the odds readout PokerTH never showed.

use tg_ai::{decide, equity, outcome_distribution, Tier};
use tg_engine::hand::{Action, Hand, Payouts, SeatStatus};
use tg_engine::rng::Rng;
use tg_engine::describe_hole;

/// A brief on-table label for a seat's most recent action, with a CSS-kind class.
pub type ActionTag = (String, &'static str);

/// The human always sits in seat 0.
pub const HUMAN: usize = 0;

/// Random display names for the AI opponents (max 7 seated, pool is larger so
/// repeats are rare across sessions).
const BOT_NAMES: [&str; 32] = [
    "Axel", "Nadia", "Marco", "Priya", "Dario", "Elena", "Ivan", "Sofia", "Bruno", "Lena",
    "Cole", "Mika", "Rafa", "Tessa", "Omar", "Vera", "Nash", "Gia", "Kane", "Luca", "Rosa",
    "Finn", "Ada", "Zane", "Nina", "Reid", "Yuki", "Diego", "Mara", "Theo", "Isla", "Sven",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Waiting for the human to act.
    HumanTurn,
    /// One or more bots still have to act; the UI steps them on a timer.
    BotsActing,
    /// Hand finished; showing results until the player starts the next one.
    HandOver,
}

pub struct Game {
    pub hand: Hand,
    /// Persistent chip stacks carried between hands (index by seat).
    pub stacks: Vec<u32>,
    pub button: usize,
    pub sb: u32,
    pub bb: u32,
    /// Difficulty per seat; seat 0 (human) is ignored.
    pub tiers: Vec<Tier>,
    /// Display names per seat.
    pub names: Vec<String>,
    pub rng: Rng,
    pub phase: Phase,
    pub log: Vec<String>,
    pub last_payouts: Option<Payouts>,
    /// Human's Monte-Carlo equity for the current spot, if it's their turn.
    pub hero_equity: Option<f64>,
    /// Natural-language name of the human's current hand (or starting hand).
    pub hero_label: Option<String>,
    /// Probability the human's hand finishes as each category (by river).
    pub hero_odds: Option<[f64; 9]>,
    /// Each seat's most recent action this street, for the on-table tag.
    pub last_action: Vec<Option<ActionTag>>,
    /// At a contested showdown, each seat's final hand description (else None).
    pub showdown_hands: Vec<Option<String>>,
    /// Which seats won chips in the last settled hand.
    pub winners: Vec<bool>,
    pub hand_number: u32,
}

impl Game {
    /// Create a new game. `opponents` bots plus the human, each starting with
    /// `starting_stack` chips.
    pub fn new(
        player_name: &str,
        opponents: usize,
        starting_stack: u32,
        sb: u32,
        bb: u32,
        tier: Tier,
        seed: u64,
    ) -> Game {
        let seats = opponents + 1;
        let stacks = vec![starting_stack; seats];
        let mut rng = Rng::seed(seed);

        // Seat 0 is the human; opponents get distinct random names.
        let mut names = Vec::with_capacity(seats);
        let hero = player_name.trim();
        names.push(if hero.is_empty() { "Player".to_string() } else { hero.to_string() });
        let mut pool: Vec<&str> = BOT_NAMES.to_vec();
        rng.shuffle(&mut pool);
        for name in pool.into_iter().take(opponents) {
            names.push(name.to_string());
        }
        let tiers = vec![tier; seats];

        let button = 0;
        let hand = Hand::start(&stacks, button, sb, bb, &mut rng);

        let mut game = Game {
            hand,
            stacks,
            button,
            sb,
            bb,
            tiers,
            names,
            rng,
            phase: Phase::BotsActing,
            log: Vec::new(),
            last_payouts: None,
            hero_equity: None,
            hero_label: None,
            hero_odds: None,
            last_action: vec![None; seats],
            showdown_hands: vec![None; seats],
            winners: vec![false; seats],
            hand_number: 1,
        };
        game.log.push("New hand dealt.".to_string());
        game.after_state_change();
        game
    }

    /// Seats that still have chips to play another hand.
    fn solvent_seats(&self) -> usize {
        self.stacks.iter().filter(|&&s| s > 0).count()
    }

    /// Begin the next hand, moving the button and re-seating stacks. Busted
    /// seats (0 chips) sit out automatically via the engine.
    pub fn next_hand(&mut self) {
        if self.solvent_seats() < 2 {
            self.log.push("Not enough players with chips. Game over.".to_string());
            return;
        }
        let n = self.stacks.len();
        let mut b = (self.button + 1) % n;
        while self.stacks[b] == 0 {
            b = (b + 1) % n;
        }
        self.button = b;
        self.hand = Hand::start(&self.stacks, self.button, self.sb, self.bb, &mut self.rng);
        self.last_payouts = None;
        self.clear_hero_analysis();
        for a in self.last_action.iter_mut() {
            *a = None;
        }
        for s in self.showdown_hands.iter_mut() {
            *s = None;
        }
        for w in self.winners.iter_mut() {
            *w = false;
        }
        self.hand_number += 1;
        self.log.push(format!("--- Hand #{} ---", self.hand_number));
        self.after_state_change();
    }

    /// Apply the human's chosen action, then settle the resulting phase.
    pub fn human_action(&mut self, action: Action) {
        if self.phase != Phase::HumanTurn || self.hand.to_act != Some(HUMAN) {
            return;
        }
        self.log.push(format!("{} {}", self.names[HUMAN], describe(&action, &self.hand)));
        let street_before = self.hand.street;
        if self.hand.apply(action).is_err() {
            return;
        }
        self.record_action(HUMAN, &action, street_before);
        self.after_state_change();
    }

    /// True while it's a bot's turn and the hand is live — the UI's cue to
    /// schedule another [`Game::step_bot`].
    pub fn is_bot_turn(&self) -> bool {
        self.phase == Phase::BotsActing
    }

    /// Perform exactly one bot's action. Returns the action taken (for the UI to
    /// sound), or `None` if it wasn't a bot's turn.
    pub fn step_bot(&mut self) -> Option<Action> {
        if self.hand.is_over() {
            self.after_state_change();
            return None;
        }
        let seat = self.hand.to_act?;
        if seat == HUMAN {
            return None;
        }
        let tier = self.tiers[seat];
        let action = decide(&self.hand, seat, tier, &mut self.rng);
        self.log.push(format!("{} {}", self.names[seat], describe(&action, &self.hand)));
        let street_before = self.hand.street;
        if self.hand.apply(action).is_err() {
            let _ = self.hand.apply(Action::Fold);
        }
        self.record_action(seat, &action, street_before);
        self.after_state_change();
        Some(action)
    }

    /// Store a seat's action as an on-table tag. When the action closes a
    /// street (the board advances), clear every tag so labels never linger from
    /// a previous street.
    fn record_action(&mut self, seat: usize, action: &Action, street_before: tg_engine::hand::Street) {
        if self.hand.street != street_before {
            for a in self.last_action.iter_mut() {
                *a = None;
            }
            return;
        }
        let all_in = self.hand.seats[seat].status == SeatStatus::AllIn;
        let tag: ActionTag = match action {
            Action::Fold => ("Fold".into(), "fold"),
            Action::Check => ("Check".into(), "check"),
            Action::Call if all_in => ("All-in".into(), "allin"),
            Action::Call => ("Call".into(), "call"),
            Action::Raise { .. } if all_in => ("All-in".into(), "allin"),
            Action::Raise { .. } => ("Raise".into(), "raise"),
        };
        self.last_action[seat] = Some(tag);
    }

    /// Recompute the phase after any state change: end the hand, hand the turn
    /// to the human (computing equity), or leave bots to act.
    fn after_state_change(&mut self) {
        if self.hand.is_over() || self.hand.to_act.is_none() {
            self.finish_hand();
        } else if self.hand.to_act == Some(HUMAN) {
            self.phase = Phase::HumanTurn;
            self.compute_hero_analysis();
        } else {
            self.phase = Phase::BotsActing;
        }
    }

    fn finish_hand(&mut self) {
        if self.phase == Phase::HandOver {
            return;
        }
        let payouts = self.hand.settle();
        for (i, seat) in self.hand.seats.iter().enumerate() {
            self.stacks[i] = seat.stack;
        }
        // A contested showdown reaches the river with 2+ players still in; only
        // then did a winner actually show a hand worth naming.
        let contested = self.hand.seats.iter().filter(|s| s.in_hand()).count() >= 2
            && self.hand.board.len() == 5;

        // Clear stale action tags and record who won + each shown hand, so the
        // table can display final hands at showdown.
        for a in self.last_action.iter_mut() {
            *a = None;
        }
        for i in 0..self.hand.seats.len() {
            self.winners[i] = payouts.winnings.get(i).is_some_and(|&w| w > 0);
            self.showdown_hands[i] = if contested && self.hand.seats[i].in_hand() {
                self.hand.seats[i].hole.map(|hole| {
                    let mut seven = self.hand.board.clone();
                    seven.push(hole[0]);
                    seven.push(hole[1]);
                    tg_engine::describe(&tg_engine::eval7(&seven))
                })
            } else {
                None
            };
        }
        for (i, &w) in payouts.winnings.iter().enumerate() {
            if w == 0 {
                continue;
            }
            let name = &self.names[i];
            let with = contested
                .then(|| self.hand.seats[i].hole)
                .flatten()
                .map(|hole| {
                    let mut seven = self.hand.board.clone();
                    seven.push(hole[0]);
                    seven.push(hole[1]);
                    format!(" with {}", lower_first(tg_engine::describe(&tg_engine::eval7(&seven))))
                })
                .unwrap_or_default();
            self.log.push(format!("{name} wins {w}{with}"));
        }
        self.last_payouts = Some(payouts);
        self.clear_hero_analysis();
        self.phase = Phase::HandOver;
    }

    fn clear_hero_analysis(&mut self) {
        self.hero_equity = None;
        self.hero_label = None;
        self.hero_odds = None;
    }

    /// Compute the human's decision aids: win equity, current hand name, and the
    /// odds of finishing as each category — the PokerTH-style readout.
    fn compute_hero_analysis(&mut self) {
        let Some(hole) = self.hand.seats[HUMAN].hole else { return };
        let board = &self.hand.board;
        let opponents = self
            .hand
            .seats
            .iter()
            .enumerate()
            .filter(|(i, s)| *i != HUMAN && s.in_hand())
            .count();

        self.hero_equity = Some(equity(hole, board, opponents.max(1), 800, &mut self.rng));

        // With a made board (5+ cards total) name the current hand; pre-flop,
        // name the starting hand. Then append any live draws.
        let mut label = if board.len() >= 3 {
            let mut seven = board.clone();
            seven.push(hole[0]);
            seven.push(hole[1]);
            tg_engine::describe(&tg_engine::eval7(&seven))
        } else {
            describe_hole(hole)
        };
        let draws = tg_engine::draws(hole, board);
        if !draws.is_empty() {
            label.push_str(" · ");
            label.push_str(&draws.join(", "));
        }
        self.hero_label = Some(label);

        self.hero_odds = Some(outcome_distribution(hole, board, 2500, &mut self.rng));
    }

    pub fn is_game_over(&self) -> bool {
        self.solvent_seats() < 2
    }
}

/// Lowercase just the first character, so a capitalized hand name reads
/// naturally mid-sentence ("wins with pair of Kings").
fn lower_first(s: String) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => s,
    }
}

/// Human-readable action, resolving `Call`/`Raise` amounts against the hand.
fn describe(action: &Action, hand: &Hand) -> String {
    match action {
        Action::Fold => "folds".to_string(),
        Action::Check => "checks".to_string(),
        Action::Call => {
            let cost = hand.legal_actions().map(|l| l.call_cost).unwrap_or(0);
            format!("calls {cost}")
        }
        Action::Raise { to } => format!("raises to {to}"),
    }
}
