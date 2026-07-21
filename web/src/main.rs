//! Tilted Gorilla — browser poker table.
//!
//! A Leptos (CSR/WASM) front end over the `tg-engine` rules and `tg-ai` bots.
//! Seat 0 is the human; everyone else is a tunable AI. The whole game state
//! lives in one signal; every action mutates it and the view re-renders.

use leptos::prelude::*;

use tg_ai::Tier;
use tg_engine::card::{Card, Suit};
use tg_engine::hand::{Action, SeatStatus, Street};

mod game;
use game::{Game, Phase, HUMAN};

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// A fresh RNG seed from the wall clock — good enough to make each session's
/// deals unpredictable.
fn seed() -> u64 {
    js_sys::Date::now() as u64 ^ 0x9E37_79B9_7F4A_7C15
}

#[component]
fn App() -> impl IntoView {
    let game = RwSignal::new(None::<Game>);
    let bet_amount = RwSignal::new(0u32);

    // Setup form state.
    let opponents = RwSignal::new(3usize);
    let difficulty = RwSignal::new(Tier::Grinder);
    let stack = RwSignal::new(1000u32);
    let big_blind = RwSignal::new(10u32);

    // When it becomes the human's turn, default the raise slider to the minimum
    // legal raise so the control starts somewhere sensible.
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
    };

    view! {
        <div class="app">
            <header class="topbar">
                <h1>"🦍 Tilted Gorilla"</h1>
                <span class="tagline">"No-Limit Hold'em — you vs. the bots"</span>
            </header>
            {move || {
                if game.with(Option::is_none) {
                    setup_view(opponents, difficulty, stack, big_blind, deal).into_any()
                } else {
                    table_view(game, bet_amount).into_any()
                }
            }}
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

/// Render one card face-up.
fn card_face(card: Card) -> impl IntoView {
    let red = matches!(card.suit, Suit::Hearts | Suit::Diamonds);
    view! {
        <div class="card" class:red=red>
            <span class="rank">{card.rank_label()}</span>
            <span class="suit">{card.suit.symbol().to_string()}</span>
        </div>
    }
}

/// A face-down card back.
fn card_back() -> impl IntoView {
    view! { <div class="card back"></div> }
}

/// The live poker table.
fn table_view(game: RwSignal<Option<Game>>, bet_amount: RwSignal<u32>) -> impl IntoView {
    // Everything reactive to the game signal lives inside this closure.
    let board = move || {
        game.with(|opt| {
            let g = opt.as_ref().unwrap();
            let mut cards: Vec<_> = g.hand.board.iter().map(|c| card_face(*c).into_any()).collect();
            // Pad to five slots for a stable layout.
            while cards.len() < 5 {
                cards.push(view! { <div class="card slot"></div> }.into_any());
            }
            cards
        })
    };

    let opponents = move || {
        game.with(|opt| {
            let g = opt.as_ref().unwrap();
            let showdown = g.phase == Phase::HandOver;
            (1..g.hand.seats.len())
                .map(|i| {
                    let seat = &g.hand.seats[i];
                    let name = g.names[i].clone();
                    let is_button = i == g.button;
                    let is_turn = g.hand.to_act == Some(i);
                    let status = seat.status;
                    let stack = seat.stack;
                    let bet = seat.street_bet;
                    let cards = match (seat.hole, status) {
                        (Some(h), SeatStatus::Folded) => {
                            let _ = h;
                            vec![].into_iter().map(|c: Card| card_face(c).into_any()).collect::<Vec<_>>()
                        }
                        (Some(h), _) if showdown && status != SeatStatus::Folded => {
                            vec![card_face(h[0]).into_any(), card_face(h[1]).into_any()]
                        }
                        (Some(_), _) => vec![card_back().into_any(), card_back().into_any()],
                        (None, _) => vec![],
                    };
                    view! {
                        <div class="seat opp"
                            class:folded=move || status == SeatStatus::Folded
                            class:active-turn=is_turn
                        >
                            <div class="seat-head">
                                <span class="name">{name}</span>
                                {is_button.then(|| view!{ <span class="btn-chip">"D"</span> })}
                            </div>
                            <div class="hole">{cards}</div>
                            <div class="seat-foot">
                                <span class="stack">{stack} " chips"</span>
                                {(bet > 0).then(|| view!{ <span class="bet">"bet " {bet}</span> })}
                                {status_badge(status)}
                            </div>
                        </div>
                    }
                })
                .collect::<Vec<_>>()
        })
    };

    let hero = move || {
        game.with(|opt| {
            let g = opt.as_ref().unwrap();
            let seat = &g.hand.seats[HUMAN];
            match seat.hole {
                Some(h) => vec![card_face(h[0]).into_any(), card_face(h[1]).into_any()],
                None => vec![],
            }
        })
    };

    view! {
        <div class="table-wrap">
            <div class="felt">
                <div class="opponents">{opponents}</div>

                <div class="center">
                    <div class="pot">
                        "Pot: " {move || game.with(|o| o.as_ref().unwrap().hand.pot_total())}
                    </div>
                    <div class="board">{board}</div>
                    <div class="street">
                        {move || game.with(|o| street_name(o.as_ref().unwrap().hand.street))}
                    </div>
                </div>

                <div class="seat hero">
                    <div class="seat-head">
                        <span class="name">"You"</span>
                        {move || game.with(|o| {
                            let g = o.as_ref().unwrap();
                            (HUMAN == g.button).then(|| view!{ <span class="btn-chip">"D"</span> })
                        })}
                    </div>
                    <div class="hole">{hero}</div>
                    <div class="seat-foot">
                        <span class="stack">
                            {move || game.with(|o| o.as_ref().unwrap().hand.seats[HUMAN].stack)} " chips"
                        </span>
                    </div>
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
                            }>"Next hand"</button>
                        }.into_any()
                    }}
                </div>
            }.into_any();
        }

        // Human's turn: show legal actions.
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

        view! {
            <div class="controls">
                <button class="act fold" on:click=move |_| {
                    game.update(|o| { if let Some(g) = o { g.human_action(Action::Fold); } });
                }>"Fold"</button>

                {if can_check {
                    view!{
                        <button class="act check" on:click=move |_| {
                            game.update(|o| { if let Some(g) = o { g.human_action(Action::Check); } });
                        }>"Check"</button>
                    }.into_any()
                } else {
                    view!{
                        <button class="act call" on:click=move |_| {
                            game.update(|o| { if let Some(g) = o { g.human_action(Action::Call); } });
                        }>"Call " {call_cost}</button>
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
                                game.update(|o| { if let Some(g) = o { g.human_action(Action::Raise { to }); } });
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
