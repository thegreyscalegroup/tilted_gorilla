//! Correctness tests for the core engine. These are the proof that the poker
//! rules are right *before* any UI exists.

use crate::card::Card;
use crate::deck::Deck;
use crate::eval::{eval5, eval7, Category};
use crate::hand::{Action, Hand, SeatStatus, Street};
use crate::pot::build_pots;
use crate::rng::Rng;

/// Parse a space-separated board like `"As Ks Qs Js Ts"` into cards.
fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace()
        .map(|c| Card::parse(c).unwrap_or_else(|| panic!("bad card {c:?}")))
        .collect()
}

fn five(s: &str) -> [Card; 5] {
    let v = cards(s);
    [v[0], v[1], v[2], v[3], v[4]]
}

// ---- Category detection -------------------------------------------------

#[test]
fn detects_each_category() {
    assert_eq!(eval5(&five("As Ks Qs Js Ts")).category, Category::StraightFlush);
    assert_eq!(eval5(&five("9h 9c 9s 9d 2h")).category, Category::FourOfAKind);
    assert_eq!(eval5(&five("9h 9c 9s 2d 2h")).category, Category::FullHouse);
    assert_eq!(eval5(&five("As Ks Qs Js 9s")).category, Category::Flush);
    assert_eq!(eval5(&five("9h 8c 7s 6d 5h")).category, Category::Straight);
    assert_eq!(eval5(&five("9h 9c 9s 5d 2h")).category, Category::ThreeOfAKind);
    assert_eq!(eval5(&five("9h 9c 5s 5d 2h")).category, Category::TwoPair);
    assert_eq!(eval5(&five("9h 9c 7s 5d 2h")).category, Category::OnePair);
    assert_eq!(eval5(&five("Ah Kc 9s 5d 2h")).category, Category::HighCard);
}

#[test]
fn wheel_is_a_five_high_straight() {
    let wheel = eval5(&five("Ah 2c 3s 4d 5h"));
    assert_eq!(wheel.category, Category::Straight);
    assert_eq!(wheel.tiebreak[0], 5, "wheel high card should be 5, not the ace");

    // A straight to 6 must beat the wheel.
    let six_high = eval5(&five("2h 3c 4s 5d 6h"));
    assert!(six_high > wheel);
}

#[test]
fn royal_flush_is_the_nuts() {
    let royal = eval5(&five("As Ks Qs Js Ts"));
    let steel_wheel = eval5(&five("As 2s 3s 4s 5s")); // 5-high straight flush
    assert!(royal > steel_wheel);
    assert_eq!(steel_wheel.tiebreak[0], 5);
}

// ---- Ordering within a category ----------------------------------------

#[test]
fn kickers_break_ties() {
    // Same pair of kings, different kicker.
    let ak = eval5(&five("Kh Kc Ah 5d 2h"));
    let kq = eval5(&five("Kh Kc Qh 5d 2h"));
    assert!(ak > kq, "ace kicker beats queen kicker");

    // Higher pair wins regardless of kickers.
    let aces = eval5(&five("Ah Ac 3h 4d 5c"));
    let kings = eval5(&five("Kh Kc Qh Jd Tc"));
    assert!(aces > kings);
}

#[test]
fn full_house_ranks_by_trips_then_pair() {
    let aces_full = eval5(&five("Ah Ac As Kd Kh"));
    let kings_full = eval5(&five("Kh Kc Ks Ad Ah"));
    assert!(aces_full > kings_full, "aces full beats kings full");
}

#[test]
fn two_pair_ranks_high_pair_then_low_then_kicker() {
    let aces_up = eval5(&five("Ah Ac 5s 5d 2h"));
    let kings_up = eval5(&five("Kh Kc Qs Qd Jh"));
    assert!(aces_up > kings_up);

    // Same two pair, kicker decides.
    let better_kicker = eval5(&five("Ah Ac 5s 5d Kh"));
    let worse_kicker = eval5(&five("Ah Ac 5s 5d 2h"));
    assert!(better_kicker > worse_kicker);
}

// ---- eval7: best five of seven -----------------------------------------

#[test]
fn eval7_picks_the_best_five() {
    // Seven cards containing a flush that must be found among noise.
    let seven = cards("As Ks 2h 7s 9s Jd 4s"); // five spades → ace-high flush
    let v = eval7(&seven);
    assert_eq!(v.category, Category::Flush);
    assert_eq!(v.tiebreak, [14, 13, 9, 7, 4]);
}

#[test]
fn eval7_finds_straight_using_both_hole_and_board() {
    // Board + hole make a 9-high straight (5 6 7 8 9), with distractor pairs.
    let seven = cards("9h 8c 7s 6d 5h Ah Ac");
    let v = eval7(&seven);
    assert_eq!(v.category, Category::Straight);
    assert_eq!(v.tiebreak[0], 9);
}

#[test]
fn eval7_prefers_full_house_over_flush_when_stronger() {
    // Full house should outrank a made flush on the same seven cards.
    let seven = cards("Ah Ad As Kh Kd 2h 3h");
    let v = eval7(&seven);
    assert_eq!(v.category, Category::FullHouse);
    assert_eq!(v.tiebreak[0], 14);
    assert_eq!(v.tiebreak[1], 13);
}

// ---- Deck & RNG ---------------------------------------------------------

#[test]
fn deck_has_52_unique_cards() {
    let mut deck = Deck::standard();
    assert_eq!(deck.remaining(), 52);
    let mut seen = std::collections::HashSet::new();
    while let Some(c) = deck.deal() {
        assert!(seen.insert((c.rank, c.suit)), "duplicate card {c}");
    }
    assert_eq!(seen.len(), 52);
}

#[test]
fn shuffle_is_deterministic_for_a_given_seed() {
    let mut a = Deck::standard();
    let mut b = Deck::standard();
    a.shuffle(&mut Rng::seed(42));
    b.shuffle(&mut Rng::seed(42));
    // Deal both fully and compare order.
    let mut order_a = Vec::new();
    let mut order_b = Vec::new();
    while let (Some(x), Some(y)) = (a.deal(), b.deal()) {
        order_a.push((x.rank, x.suit));
        order_b.push((y.rank, y.suit));
    }
    assert_eq!(order_a, order_b);

    // A different seed should (almost surely) produce a different order.
    let mut c = Deck::standard();
    c.shuffle(&mut Rng::seed(43));
    let mut order_c = Vec::new();
    while let Some(x) = c.deal() {
        order_c.push((x.rank, x.suit));
    }
    assert_ne!(order_a, order_c);
}

#[test]
fn rng_below_stays_in_range() {
    let mut rng = Rng::seed(7);
    for _ in 0..10_000 {
        let v = rng.below(6);
        assert!(v < 6);
    }
}

#[test]
fn shuffle_still_yields_52_unique_cards() {
    let mut deck = Deck::standard();
    deck.shuffle(&mut Rng::seed(999));
    let mut seen = std::collections::HashSet::new();
    while let Some(c) = deck.deal() {
        assert!(seen.insert((c.rank, c.suit)));
    }
    assert_eq!(seen.len(), 52);
}

// ---- Side pots ----------------------------------------------------------

#[test]
fn side_pots_single_pot_when_contributions_equal() {
    let pots = build_pots(&[100, 100, 100], &[false, false, false]);
    assert_eq!(pots.len(), 1);
    assert_eq!(pots[0].amount, 300);
    assert_eq!(pots[0].eligible, vec![0, 1, 2]);
}

#[test]
fn side_pots_split_on_all_in_shortstack() {
    // Seat 0 all-in for 50, seats 1 & 2 contest 100 each.
    // Main pot: 50*3 = 150, eligible everyone.
    // Side pot: 50*2 = 100, eligible seats 1 & 2 only.
    let pots = build_pots(&[50, 100, 100], &[false, false, false]);
    assert_eq!(pots.len(), 2);
    assert_eq!(pots[0].amount, 150);
    assert_eq!(pots[0].eligible, vec![0, 1, 2]);
    assert_eq!(pots[1].amount, 100);
    assert_eq!(pots[1].eligible, vec![1, 2]);
}

#[test]
fn folded_player_funds_pot_but_cannot_win() {
    // Seat 2 folded after putting in 100. Chips stay in the pot; seat 2 is not
    // eligible.
    let pots = build_pots(&[100, 100, 100], &[false, false, true]);
    assert_eq!(pots.len(), 1);
    assert_eq!(pots[0].amount, 300);
    assert_eq!(pots[0].eligible, vec![0, 1]);
}

// ---- Hand flow ----------------------------------------------------------

#[test]
fn heads_up_blinds_and_first_to_act() {
    let mut rng = Rng::seed(1);
    let hand = Hand::start(&[1000, 1000], 0, 5, 10, &mut rng);
    // Heads-up: button (seat 0) posts small blind and acts first preflop.
    assert_eq!(hand.seats[0].committed, 5);
    assert_eq!(hand.seats[1].committed, 10);
    assert_eq!(hand.current_bet, 10);
    assert_eq!(hand.to_act, Some(0));
    // Both were dealt two hole cards.
    assert!(hand.seats[0].hole.is_some());
    assert!(hand.seats[1].hole.is_some());
}

#[test]
fn fold_ends_hand_and_awards_pot() {
    let mut rng = Rng::seed(2);
    let mut hand = Hand::start(&[1000, 1000], 0, 5, 10, &mut rng);
    // Button/SB folds preflop; big blind wins the 15 in the pot.
    hand.apply(Action::Fold).unwrap();
    assert!(hand.is_over());
    let payouts = hand.settle();
    assert_eq!(payouts.winnings[1], 15);
    // Winner's stack: started 1000, posted 10 blind, back to 1005 net (+5).
    assert_eq!(hand.seats[1].stack, 1005);
    assert_eq!(hand.seats[0].stack, 995);
}

#[test]
fn full_hand_runs_to_showdown_and_conserves_chips() {
    let mut rng = Rng::seed(12345);
    let starting = [1000u32, 1000, 1000];
    let mut hand = Hand::start(&starting, 0, 5, 10, &mut rng);

    // Everyone just calls/checks through every street until showdown.
    let mut guard = 0;
    while !hand.is_over() {
        let legal = hand.legal_actions().expect("someone must be on turn");
        let action = if legal.can_check {
            Action::Check
        } else {
            Action::Call
        };
        hand.apply(action).unwrap();
        guard += 1;
        assert!(guard < 100, "hand failed to terminate");
    }

    assert_eq!(hand.street, Street::Showdown);
    assert_eq!(hand.board.len(), 5, "full board dealt by showdown");

    let payouts = hand.settle();
    // Chip conservation: total stacks after == total chips before.
    let total_after: u32 = hand.seats.iter().map(|s| s.stack).sum();
    assert_eq!(total_after, starting.iter().sum::<u32>());
    // Someone won the pot.
    assert!(payouts.winnings.iter().any(|&w| w > 0));
}

#[test]
fn all_in_preflop_runs_out_board_and_conserves_chips() {
    let mut rng = Rng::seed(777);
    let starting = [40u32, 1000, 1000];
    let mut hand = Hand::start(&starting, 0, 5, 10, &mut rng);

    // Drive to completion: raise all-in when possible, otherwise call.
    let mut guard = 0;
    while !hand.is_over() {
        let legal = hand.legal_actions().unwrap();
        let action = if legal.can_raise {
            Action::Raise { to: legal.max_raise_to }
        } else if legal.call_cost > 0 {
            Action::Call
        } else {
            Action::Check
        };
        hand.apply(action).unwrap();
        guard += 1;
        assert!(guard < 100, "hand failed to terminate");
    }

    // Short stack should be all-in, board fully run out.
    assert_eq!(hand.board.len(), 5);
    hand.settle();
    let total_after: u32 = hand.seats.iter().map(|s| s.stack).sum();
    assert_eq!(total_after, starting.iter().sum::<u32>());
}

#[test]
fn big_blind_gets_option_to_raise_when_limped_to() {
    let mut rng = Rng::seed(55);
    // 3-handed so there's an under-the-gun caller then folds around to BB.
    let mut hand = Hand::start(&[1000, 1000, 1000], 0, 5, 10, &mut rng);
    // Seat order: button=0, SB=1, BB=2, UTG acts first = seat 0.
    assert_eq!(hand.to_act, Some(0));
    hand.apply(Action::Call).unwrap(); // seat 0 limps
    hand.apply(Action::Call).unwrap(); // seat 1 (SB) completes
    // Now action is on the big blind (seat 2) with the option.
    assert_eq!(hand.to_act, Some(2));
    let legal = hand.legal_actions().unwrap();
    assert!(legal.can_check, "BB can check its option");
    assert!(legal.can_raise, "BB can raise its option");
    // BB checks → street should complete and move to the flop.
    hand.apply(Action::Check).unwrap();
    assert_eq!(hand.street, Street::Flop);
    assert_eq!(hand.board.len(), 3);
}

#[test]
fn cannot_check_facing_a_bet() {
    let mut rng = Rng::seed(9);
    let mut hand = Hand::start(&[1000, 1000], 0, 5, 10, &mut rng);
    // Button/SB owes 5 to call; checking must be rejected.
    let err = hand.apply(Action::Check);
    assert!(err.is_err());
    // State unchanged: still seat 0 to act.
    assert_eq!(hand.to_act, Some(0));
    assert_eq!(hand.seats[0].status, SeatStatus::Active);
}
