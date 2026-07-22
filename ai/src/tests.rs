//! AI sanity tests. Monte-Carlo results carry variance, so these assert on
//! generous ranges rather than exact values — enough to prove the equity math
//! and the policy's judgement are sound.

use tg_engine::card::Card;
use tg_engine::eval::Category;
use tg_engine::hand::{Action, Hand};
use tg_engine::rng::Rng;

use crate::equity::equity;
use crate::odds::outcome_distribution;
use crate::policy::{decide, Tier};

fn card(s: &str) -> Card {
    Card::parse(s).unwrap()
}

fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace().map(card).collect()
}

// ---- Equity -------------------------------------------------------------

#[test]
fn pocket_aces_crush_heads_up_preflop() {
    let mut rng = Rng::seed(1);
    let eq = equity([card("As"), card("Ah")], &[], 1, 3000, &mut rng);
    // AA is ~85% heads-up preflop.
    assert!(eq > 0.80 && eq < 0.90, "AA equity was {eq}");
}

#[test]
fn worst_hand_is_an_underdog_to_aces() {
    let mut rng = Rng::seed(2);
    let seven_two = equity([card("7d"), card("2c")], &[], 1, 3000, &mut rng);
    assert!(seven_two < 0.40, "72o equity was {seven_two}");
}

#[test]
fn made_nut_flush_on_river_is_near_certain() {
    let mut rng = Rng::seed(3);
    // Hero holds nut flush; board has three more spades and no pair (no full
    // house possible for a single opponent to beat the flush easily).
    let board = cards("Ks Qs 2s 7h 8d");
    let eq = equity([card("As"), card("Js")], &board, 1, 3000, &mut rng);
    assert!(eq > 0.95, "nut flush equity was {eq}");
}

#[test]
fn more_opponents_lowers_equity() {
    let mut rng = Rng::seed(4);
    let hero = [card("As"), card("Ah")];
    let heads_up = equity(hero, &[], 1, 3000, &mut rng);
    let five_way = equity(hero, &[], 5, 3000, &mut rng);
    assert!(five_way < heads_up, "{five_way} should be < {heads_up}");
}

// ---- Outcome odds -------------------------------------------------------

#[test]
fn distribution_sums_to_one() {
    let mut rng = Rng::seed(1);
    let d = outcome_distribution([card("As"), card("Kd")], &cards("2h 7c 9s"), 2000, &mut rng);
    let sum: f64 = d.iter().sum();
    assert!((sum - 1.0).abs() < 1e-9, "distribution summed to {sum}");
}

#[test]
fn complete_board_is_certain() {
    let mut rng = Rng::seed(1);
    // Hero holds the nut flush on a complete board.
    let d = outcome_distribution([card("As"), card("Js")], &cards("Ks Qs 2s 7h 8d"), 50, &mut rng);
    assert_eq!(d[Category::Flush.index()], 1.0);
}

#[test]
fn turn_flush_draw_odds_are_reasonable() {
    // Four spades on the turn (one card to come): P(flush) = 9 spades / 46 unseen
    // ≈ 0.196.
    let mut rng = Rng::seed(7);
    let d = outcome_distribution([card("As"), card("Js")], &cards("Ks 2s 7h 8d"), 30000, &mut rng);
    let p = d[Category::Flush.index()];
    assert!(p > 0.15 && p < 0.24, "turn flush-draw odds were {p}");
}

// ---- Policy -------------------------------------------------------------

#[test]
fn folds_trash_to_a_big_bet() {
    // Construct a hand where the bot faces a large bet holding nothing.
    let mut rng = Rng::seed(10);
    let mut hand = Hand::start(&[1000, 1000], 0, 5, 10, &mut rng);
    // Force known hole cards: give seat-to-act 7-2 offsuit.
    hand.seats[0].hole = Some([card("7d"), card("2c")]);
    hand.seats[1].hole = Some([card("As"), card("Ah")]);
    // Seat 1 (BB) is not on turn; seat 0 acts first. Have seat 0 face a raise:
    // seat 0 raises small, seat 1 re-raises big — actually simpler: just check
    // that with trash and a call cost, a Rock folds.
    // Put a big bet on the table by having seat 0 call then seat... simplify:
    // set current_bet high via a seat-1 raise path.
    // Seat 0 to act, owes 5 to call the BB. That's cheap, so a Rock may call.
    // Instead, simulate facing a pot-sized raise: seat 0 calls, BB raises big.
    hand.apply(Action::Call).unwrap(); // seat 0 completes SB
    // Now BB (seat 1) to act with option; make it raise big.
    hand.apply(Action::Raise { to: 300 }).unwrap();
    // Back to seat 0 with trash facing a 290 call into a ~320 pot.
    assert_eq!(hand.to_act, Some(0));
    let action = decide(&hand, 0, Tier::Rock, &mut rng);
    assert_eq!(action, Action::Fold, "Rock should fold 72o to a big reraise");
}

#[test]
fn value_raises_a_monster() {
    let mut rng = Rng::seed(11);
    let mut hand = Hand::start(&[1000, 1000], 0, 5, 10, &mut rng);
    hand.seats[0].hole = Some([card("As"), card("Ah")]);
    // On turn, seat 0 with aces facing just the blind should not fold; it should
    // raise for value (or at minimum call). Assert it doesn't fold/check away.
    let action = decide(&hand, 0, Tier::Grinder, &mut rng);
    match action {
        Action::Raise { .. } | Action::Call => {}
        other => panic!("expected aces to raise/call, got {other:?}"),
    }
}

#[test]
fn a_full_bot_hand_terminates_and_conserves_chips() {
    // Two bots play a hand end to end; the game must always terminate with a
    // legal action sequence and conserve chips.
    let starting = [500u32, 500, 500];
    let mut rng = Rng::seed(2024);
    let mut hand = Hand::start(&starting, 0, 5, 10, &mut rng);

    let tiers = [Tier::Rock, Tier::Grinder, Tier::Shark];
    let mut guard = 0;
    while !hand.is_over() {
        let seat = hand.to_act.expect("someone on turn");
        let action = decide(&hand, seat, tiers[seat], &mut rng);
        hand.apply(action).expect("AI produced a legal action");
        guard += 1;
        assert!(guard < 200, "bot hand failed to terminate");
    }
    hand.settle();
    let total: u32 = hand.seats.iter().map(|s| s.stack).sum();
    assert_eq!(total, starting.iter().sum::<u32>(), "chips must be conserved");
}

#[test]
fn bots_never_produce_illegal_actions_over_many_hands() {
    // Fuzz: play many hands with rotating button and varied stacks; every action
    // the policy returns must be accepted by the engine.
    for seed in 0..40u64 {
        let mut rng = Rng::seed(seed);
        let starting = [300u32, 450, 275, 600];
        let button = (seed % 4) as usize;
        let mut hand = Hand::start(&starting, button, 5, 10, &mut rng);
        let tiers = [Tier::Shark, Tier::Grinder, Tier::Rock, Tier::Shark];
        let mut guard = 0;
        while !hand.is_over() {
            let seat = hand.to_act.unwrap();
            let action = decide(&hand, seat, tiers[seat], &mut rng);
            hand.apply(action)
                .unwrap_or_else(|e| panic!("seed {seed}: illegal action {action:?}: {e}"));
            guard += 1;
            assert!(guard < 300, "seed {seed}: hand failed to terminate");
        }
        hand.settle();
        let total: u32 = hand.seats.iter().map(|s| s.stack).sum();
        assert_eq!(total, starting.iter().sum::<u32>(), "seed {seed}: chip leak");
    }
}
