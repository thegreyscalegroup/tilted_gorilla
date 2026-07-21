//! Tilted Gorilla — browser poker table.
//!
//! A Leptos (CSR/WASM) front end over the `tg-engine` rules and `tg-ai` bots.
//! Seat 0 is the human; everyone else is a tunable AI. Bots act on a timer via
//! [`advance`] so the table plays out at a human pace, with a soft sound per
//! action and cards that animate onto the felt.

use std::time::Duration;

use leptos::prelude::*;

use tg_ai::Tier;
use tg_engine::card::{Card, Suit};
use tg_engine::hand::{Action, SeatStatus, Street};

mod audio;
mod game;
use game::{Game, Phase, HUMAN};

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// A fresh RNG seed from the wall clock.
fn seed() -> u64 {
    js_sys::Date::now() as u64 ^ 0x9E37_79B9_7F4A_7C15
}

/// Stable per-card key (0..52) so keyed rendering only animates *new* cards.
fn card_key(c: Card) -> u8 {
    let suit = match c.suit {
        Suit::Clubs => 0,
        Suit::Diamonds => 1,
        Suit::Hearts => 2,
        Suit::Spades => 3,
    };
    suit * 13 + (c.rank - 2)
}

/// Play the soft sound that matches an action.
fn play_action_sound(a: &Action) {
    match a {
        Action::Fold => audio::fold_swish(),
        Action::Check => audio::check_tap(),
        Action::Call | Action::Raise { .. } => audio::chip(),
    }
}

/// A little flurry of card whisks when a hand is dealt.
fn play_deal_sounds() {
    audio::deal_card();
    set_timeout(audio::deal_card, Duration::from_millis(95));
    set_timeout(audio::deal_card, Duration::from_millis(190));
}

/// Drive the table forward: if a bot is due, step it after a short delay (so the
/// action is watchable), play its sound, then recurse. When the hand ends, rake
/// the pot sound once.
fn advance(game: RwSignal<Option<Game>>) {
    let over = game.with(|o| o.as_ref().map_or(false, |g| g.phase == Phase::HandOver));
    if over {
        audio::pot_win();
        return;
    }
    let bot_turn = game.with(|o| o.as_ref().map_or(false, |g| g.is_bot_turn()));
    if bot_turn {
        set_timeout(
            move || {
                let acted = game
                    .try_update(|o| o.as_mut().and_then(Game::step_bot))
                    .flatten();
                if let Some(a) = acted {
                    play_action_sound(&a);
                }
                advance(game);
            },
            Duration::from_millis(650),
        );
    }
}

#[component]
fn App() -> impl IntoView {
    let game = RwSignal::new(None::<Game>);
    let bet_amount = RwSignal::new(0u32);
    let muted = RwSignal::new(false);

    // Setup form state.
    let opponents = RwSignal::new(3usize);
    let difficulty = RwSignal::new(Tier::Grinder);
    let stack = RwSignal::new(1000u32);
    let big_blind = RwSignal::new(10u32);

    // Default the raise slider to the minimum legal raise whenever it becomes
    // the human's turn.
    Effect::new(move |_| {
        game.with(|opt| {
            if let Some(g) = opt {
                if g.phase == Phase::HumanTurn {
                    if let Some(l) = g.hand.legal_actions() {
                        if l.can_raise {
                            bet_amount.set(l.min_raise_to);
                        }
                    }
                }
            }
        });
    });

    let deal = move |_| {
        let g = Game::new(
            opponents.get(),
            stack.get(),
            (big_blind.get() / 2).max(1),
            big_blind.get(),
            difficulty.get(),
            seed(),
        );
        game.set(Some(g));
        play_deal_sounds();
        advance(game);
    };

    view! {
        <div class="app">
            <header class="topbar">
                <h1>"🦍 Tilted Gorilla"</h1>
                <button class="mute" on:click=move |_| {
                    let m = !muted.get();
                    muted.set(m);
                    audio::set_muted(m);
                }>
                    {move || if muted.get() { "🔇" } else { "🔊" }}
                </button>
            </header>
            {
                // Only re-run this switch when a game appears/disappears — NOT
                // on every action. A plain `game.with(...)` here would rebuild
                // the whole table (and re-animate every card) on each move.
                let has_game = Memo::new(move |_| game.with(Option::is_some));
                move || {
                    if has_game.get() {
                        table_view(game, bet_amount).into_any()
                    } else {
                        setup_view(opponents, difficulty, stack, big_blind, deal).into_any()
                    }
                }
            }
        </div>
    }
}

/// The pre-game configuration screen.
fn setup_view(
    opponents: RwSignal<usize>,
    difficulty: RwSignal<Tier>,
    stack: RwSignal<u32>,
    big_blind: RwSignal<u32>,
    deal: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    let tier_btn = move |t: Tier, label: &'static str, blurb: &'static str| {
        view! {
            <button
                class="tier"
                class:selected=move || difficulty.get() == t
                on:click=move |_| difficulty.set(t)
            >
                <strong>{label}</strong>
                <span>{blurb}</span>
            </button>
        }
    };

    view! {
        <div class="setup">
            <h2>"Table setup"</h2>

            <label class="field">
                <span>"Opponents: " {move || opponents.get()}</span>
                <input type="range" min="1" max="7"
                    prop:value=move || opponents.get().to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse() { opponents.set(v); }
                    }
                />
            </label>

            <label class="field">
                <span>"Starting stack: " {move || stack.get()}</span>
                <input type="range" min="200" max="5000" step="100"
                    prop:value=move || stack.get().to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse() { stack.set(v); }
                    }
                />
            </label>

            <label class="field">
                <span>"Big blind: " {move || big_blind.get()}</span>
                <input type="range" min="2" max="100" step="2"
                    prop:value=move || big_blind.get().to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse() { big_blind.set(v); }
                    }
                />
            </label>

            <div class="tiers">
                {tier_btn(Tier::Rock, "Rock", "Tight & passive. Beatable.")}
                {tier_btn(Tier::Grinder, "Grinder", "Solid TAG. Fair fight.")}
                {tier_btn(Tier::Shark, "Shark", "Aggressive & tricky.")}
            </div>

            <button class="deal" on:click=deal>"Deal me in"</button>
        </div>
    }
}

/// Render one card face-up — casino style, with rank+suit indices in opposite
/// corners and a large center pip.
fn card_face(card: Card) -> impl IntoView {
    let red = matches!(card.suit, Suit::Hearts | Suit::Diamonds);
    let r = card.rank_label();
    let s = card.suit.symbol().to_string();
    view! {
        <div class="card" class:red=red>
            <div class="corner tl"><span class="cr">{r}</span><span class="cs">{s.clone()}</span></div>
            <div class="pip">{s.clone()}</div>
            <div class="corner br"><span class="cr">{r}</span><span class="cs">{s}</span></div>
        </div>
    }
}

/// A face-down card back with a patterned reverse.
fn card_back() -> impl IntoView {
    view! { <div class="card back"><div class="back-emblem">"🦍"</div></div> }
}

/// Absolute position (as a `left`/`top` style string) for a seat placed around
/// the oval. Seat 0 (the human) sits at the bottom; the rest spread evenly
/// around the ellipse.
fn seat_angle(i: usize, total: usize) -> f64 {
    // Seat 0 at the bottom (90°), others spaced evenly clockwise.
    std::f64::consts::PI * (0.5 + 2.0 * i as f64 / total.max(2) as f64)
}

fn seat_style(i: usize, total: usize) -> String {
    let angle = seat_angle(i, total);
    // Ellipse tuned to sit seats on the felt just inside the photographic table's
    // rail (the green playable area is inset from the image edges). Pushed a
    // little wider/taller so the top-center seat's cards clear the pot.
    let (rx, ry) = (37.0, 28.0);
    let (cx, cy) = (50.0, 48.0);
    let left = cx + rx * angle.cos();
    let top = cy + ry * angle.sin();
    format!("left:{left:.1}%;top:{top:.1}%")
}

/// Whether a seat sits in the upper half of the table (so its cards should hang
/// toward the center rather than rise into the rail).
fn seat_is_upper(i: usize, total: usize) -> bool {
    seat_angle(i, total).sin() < -0.3
}

/// A small chip graphic showing a bet/pot amount.
fn chip(amount: u32, class: &'static str) -> impl IntoView {
    let class = format!("chip {class}");
    view! {
        <div class=class>
            <span class="chip-ring"></span>
            <span class="chip-amt">{amount}</span>
        </div>
    }
}

/// Opponent seat indices (1..count). A named helper so the `view!` macro never
/// sees a `collect::<Vec<_>>()` turbofish — its angle brackets confuse the
/// macro's tag parser.
fn seat_indices(count: usize) -> Vec<usize> {
    (1..count).collect()
}

fn board_cards(game: RwSignal<Option<Game>>) -> Vec<Card> {
    game.with(|o| o.as_ref().map(|g| g.hand.board.clone()).unwrap_or_default())
}

fn hero_cards(game: RwSignal<Option<Game>>) -> Vec<Card> {
    game.with(|o| {
        o.as_ref()
            .and_then(|g| g.hand.seats[HUMAN].hole)
            .map(|h| h.to_vec())
            .unwrap_or_default()
    })
}

/// What an opponent's hole cards should show. Kept as a `PartialEq` value so a
/// per-seat `Memo` only rebuilds the card DOM when it actually changes (deal,
/// fold, showdown) — not on every betting action.
#[derive(Clone, PartialEq)]
enum HoleView {
    Backs,
    Empty,
    Faces(Card, Card),
}

/// Render one opponent seat, positioned around the oval. Each dynamic field is
/// its own fine-grained reactive block, so an action only updates the bits that
/// changed — the seat container and unchanged cards stay mounted.
fn opp_seat(game: RwSignal<Option<Game>>, i: usize, total: usize) -> impl IntoView {
    let with_seat = move |f: &dyn Fn(&Game) -> String| -> String {
        game.with(|o| o.as_ref().map(|g| f(g)).unwrap_or_default())
    };

    // Memoized card state so the hole only re-renders on real changes.
    let hole = Memo::new(move |_| {
        game.with(|o| {
            let Some(g) = o.as_ref() else { return HoleView::Empty };
            let seat = &g.hand.seats[i];
            match (seat.hole, seat.status) {
                (_, SeatStatus::Folded) | (None, _) => HoleView::Empty,
                (Some(h), _) if g.phase == Phase::HandOver => HoleView::Faces(h[0], h[1]),
                (Some(_), _) => HoleView::Backs,
            }
        })
    });

    view! {
        <div class="seat opp" style=seat_style(i, total) class:upper=seat_is_upper(i, total)
            class:folded=move || game.with(|o| o.as_ref().map_or(false, |g| g.hand.seats[i].status == SeatStatus::Folded))
            class:active-turn=move || game.with(|o| o.as_ref().map_or(false, |g| g.hand.to_act == Some(i)))
        >
            <div class="hole">
                {move || match hole.get() {
                    HoleView::Backs => vec![card_back().into_any(), card_back().into_any()],
                    HoleView::Faces(a, b) => vec![card_face(a).into_any(), card_face(b).into_any()],
                    HoleView::Empty => vec![],
                }}
            </div>
            <div class="pod">
                <div class="pod-info">
                    <span class="name">{move || with_seat(&|g| g.names[i].clone())}</span>
                    <span class="stack">{move || with_seat(&|g| g.hand.seats[i].stack.to_string())}</span>
                </div>
                {move || game.with(|o| o.as_ref().map_or(false, |g| g.button == i))
                    .then(|| view! { <span class="btn-chip">"D"</span> })}
                {move || status_badge(game.with(|o| o.as_ref().map_or(SeatStatus::SittingOut, |g| g.hand.seats[i].status)))}
            </div>
            {move || {
                let bet = game.with(|o| o.as_ref().map_or(0, |g| g.hand.seats[i].street_bet));
                (bet > 0).then(move || chip(bet, "bet").into_any()).unwrap_or_else(|| ().into_any())
            }}
        </div>
    }
}

/// The live poker table. Built once (gated by a `Memo` upstream); everything
/// that changes during play is a fine-grained reactive block inside it.
fn table_view(game: RwSignal<Option<Game>>, bet_amount: RwSignal<u32>) -> impl IntoView {
    // Seat count is fixed for the life of a game, so capture it once —
    // *untracked*, so building the table does not subscribe the enclosing
    // switch closure to the game signal (which would rebuild the whole table,
    // re-mounting every card, on each action).
    let seat_count = game.with_untracked(|o| o.as_ref().map(|g| g.hand.seats.len()).unwrap_or(0));

    let total = seat_count.max(2);

    view! {
        <div class="table-wrap">
            <div class="felt">
                <div class="felt-inner">
                    <div class="center">
                        <div class="pot-area">
                            {move || {
                                let p = game.with(|o| o.as_ref().map_or(0, |g| g.hand.pot_total()));
                                if p > 0 { chip(p, "pot").into_any() } else { ().into_any() }
                            }}
                            <div class="pot-label">
                                "Pot " {move || game.with(|o| o.as_ref().map_or(0, |g| g.hand.pot_total()))}
                            </div>
                        </div>
                        <div class="board">
                            <For each=move || board_cards(game) key=|c| card_key(*c) let:card>
                                {card_face(card)}
                            </For>
                            {move || {
                                let filled = board_cards(game).len();
                                (filled..5)
                                    .map(|_| view! { <div class="card slot"></div> })
                                    .collect_view()
                            }}
                        </div>
                        <div class="street">
                            {move || game.with(|o| street_name(o.as_ref().unwrap().hand.street))}
                        </div>
                    </div>

                    <div class="seat hero" style="left:50%;top:66%"
                        class:active-turn=move || game.with(|o| o.as_ref().map_or(false, |g| g.phase == Phase::HumanTurn))
                    >
                        <div class="hole">
                            <For each=move || hero_cards(game) key=|c| card_key(*c) let:card>
                                {card_face(card)}
                            </For>
                        </div>
                        <div class="pod">
                            <div class="pod-info">
                                <span class="name">"You"</span>
                                <span class="stack">
                                    {move || game.with(|o| o.as_ref().unwrap().hand.seats[HUMAN].stack)}
                                </span>
                            </div>
                            {move || game.with(|o| o.as_ref().map_or(false, |g| g.button == HUMAN))
                                .then(|| view! { <span class="btn-chip">"D"</span> })}
                        </div>
                        {move || {
                            let bet = game.with(|o| o.as_ref().map_or(0, |g| g.hand.seats[HUMAN].street_bet));
                            if bet > 0 { chip(bet, "bet").into_any() } else { ().into_any() }
                        }}
                    </div>

                    <For each=move || seat_indices(seat_count) key=|i| *i let:i>
                        {opp_seat(game, i, total)}
                    </For>
                </div>
            </div>

            {move || equity_meter(game)}
            {move || controls(game, bet_amount)}
            {move || action_log(game)}
        </div>
    }
}

/// Win-probability meter — the odds readout PokerTH never offered.
fn equity_meter(game: RwSignal<Option<Game>>) -> impl IntoView {
    game.with(|opt| {
        let g = opt.as_ref().unwrap();
        match g.hero_equity {
            Some(eq) => {
                let pct = (eq * 100.0).round() as u32;
                view! {
                    <div class="equity">
                        <span class="eq-label">"Your equity"</span>
                        <div class="eq-bar"><div class="eq-fill" style:width=move || format!("{pct}%")></div></div>
                        <span class="eq-pct">{pct} "%"</span>
                    </div>
                }.into_any()
            }
            None => ().into_any(),
        }
    })
}

/// The action bar (fold/check/call/raise) or the between-hands controls.
fn controls(game: RwSignal<Option<Game>>, bet_amount: RwSignal<u32>) -> impl IntoView {
    game.with(|opt| {
        let g = opt.as_ref().unwrap();

        if g.phase == Phase::HandOver {
            let over = g.is_game_over();
            return view! {
                <div class="controls">
                    {if over {
                        view!{ <div class="result">"Game over."</div> }.into_any()
                    } else {
                        view!{
                            <button class="act next" on:click=move |_| {
                                game.update(|o| { if let Some(g) = o { g.next_hand(); } });
                                play_deal_sounds();
                                advance(game);
                            }>"Next hand"</button>
                        }.into_any()
                    }}
                </div>
            }.into_any();
        }

        if g.phase == Phase::BotsActing {
            return view! { <div class="controls thinking">"Bots are acting…"</div> }.into_any();
        }

        // Human's turn.
        let Some(legal) = g.hand.legal_actions() else {
            return ().into_any();
        };
        if g.hand.to_act != Some(HUMAN) {
            return ().into_any();
        }

        let can_check = legal.can_check;
        let call_cost = legal.call_cost;
        let can_raise = legal.can_raise;
        let min_to = legal.min_raise_to;
        let max_to = legal.max_raise_to;

        // A human action: play its sound, apply it, then let the bots respond.
        let act = move |action: Action| {
            play_action_sound(&action);
            game.update(|o| { if let Some(g) = o { g.human_action(action); } });
            advance(game);
        };

        view! {
            <div class="controls">
                <button class="act fold" on:click=move |_| act(Action::Fold)>"Fold"</button>

                {if can_check {
                    view!{
                        <button class="act check" on:click=move |_| act(Action::Check)>"Check"</button>
                    }.into_any()
                } else {
                    view!{
                        <button class="act call" on:click=move |_| act(Action::Call)>"Call " {call_cost}</button>
                    }.into_any()
                }}

                {if can_raise {
                    view!{
                        <div class="raise-group">
                            <input type="range"
                                min=min_to.to_string() max=max_to.to_string()
                                prop:value=move || bet_amount.get().clamp(min_to, max_to).to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                        bet_amount.set(v.clamp(min_to, max_to));
                                    }
                                }
                            />
                            <button class="act raise" on:click=move |_| {
                                let to = bet_amount.get().clamp(min_to, max_to);
                                act(Action::Raise { to });
                            }>
                                {move || {
                                    let to = bet_amount.get().clamp(min_to, max_to);
                                    if to >= max_to { format!("All-in {to}") } else { format!("Raise to {to}") }
                                }}
                            </button>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>
        }.into_any()
    })
}

/// Scrolling text log of recent actions.
fn action_log(game: RwSignal<Option<Game>>) -> impl IntoView {
    game.with(|opt| {
        let g = opt.as_ref().unwrap();
        let lines: Vec<_> = g
            .log
            .iter()
            .rev()
            .take(10)
            .map(|l| view! { <div class="log-line">{l.clone()}</div> })
            .collect();
        view! { <div class="log">{lines}</div> }
    })
}

fn status_badge(status: SeatStatus) -> impl IntoView {
    match status {
        SeatStatus::Folded => view! { <span class="badge fold">"folded"</span> }.into_any(),
        SeatStatus::AllIn => view! { <span class="badge allin">"all-in"</span> }.into_any(),
        _ => ().into_any(),
    }
}

fn street_name(s: Street) -> &'static str {
    match s {
        Street::Preflop => "Pre-flop",
        Street::Flop => "Flop",
        Street::Turn => "Turn",
        Street::River => "River",
        Street::Showdown => "Showdown",
    }
}
