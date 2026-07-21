# 🦍 Tilted Gorilla

A browser-based No-Limit Texas Hold'em game — a modern answer to
[PokerTH](https://github.com/pokerth/pokerth). Zero install: it's a ~220 KB
static site that runs entirely in your browser via WebAssembly. Written in Rust.

## Why it's better than PokerTH

PokerTH is a fine C++/Qt desktop app, but its two most-cited weaknesses are the
AI ("shoves premium hands, insta-folds to aggression, no difficulty settings")
and the need to install it. Tilted Gorilla targets exactly those:

| | PokerTH | Tilted Gorilla |
|---|---|---|
| **Install** | Desktop/mobile installer | Open a URL — nothing to install |
| **AI** | One fixed, weak bot | Monte-Carlo equity + pot-odds policy, **3 difficulty tiers** |
| **Bluffing** | Effectively none | Controlled, tier-dependent bluff/semi-bluff frequency |
| **Odds display** | None | **Live win-% equity meter** on every decision |
| **Footprint** | Multi-MB native app | 222 KB static bundle (~80 KB gzipped) |

## Architecture

Three Rust crates, cleanly separated so the rules are proven correct before any
UI exists — and so a future multiplayer server could reuse the engine untouched.

```
tilted_gorilla/
├── engine/   # Pure rules: cards, deck, shuffle, 7-card evaluator,
│             # betting state machine, side pots. Zero dependencies. 22 tests.
├── ai/       # Monte-Carlo hand equity + tunable pot-odds policy
│             # (Rock / Grinder / Shark). 8 tests incl. a legality fuzzer.
└── web/      # Leptos (CSR) front end → WASM. Felt table, CSS cards,
              # equity meter, action log. Built with Trunk.
```

- **`engine`** knows nothing about UI, WASM, or opponents.
- **`ai`** depends only on `engine`.
- **`web`** wires them together for the browser.

## Running it

Prerequisites: a Rust toolchain, the `wasm32-unknown-unknown` target, and
[Trunk](https://trunkrs.dev/).

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk            # or grab a prebuilt binary from Trunk's releases

# Dev server with hot reload:
cd web && trunk serve --open

# Production build → static files in web/dist/ (deploy anywhere):
cd web && trunk build --release
```

## Testing the game logic

The engine and AI are validated natively (no browser needed):

```bash
cargo test          # 30 tests: hand ranking, side pots, chip conservation,
                    # equity sanity, and a fuzzer that plays 40 full hands and
                    # asserts the bots never make an illegal move.
```

## Difficulty tiers

| Tier | Style | Notes |
|---|---|---|
| **Rock** | Tight & passive | Plays strong hands, rarely bluffs. Beatable. |
| **Grinder** | Tight-aggressive | Respects pot odds, value-bets, mixes in bluffs. |
| **Shark** | Aggressive & tricky | Thin value, frequent semi-bluffs and bluff-raises. |

## Scope & roadmap

**v1 (done):** single-player vs. 1–7 AI bots, No-Limit Hold'em, configurable
blinds/stacks/difficulty, live equity, hand log — fully client-side.

**Deliberately deferred** (engine is designed to accommodate them):
online multiplayer (the engine is already server-authority-ready), tournament
structures, and other poker variants (Omaha, etc.).

## License

AGPL-3.0-or-later (matching PokerTH's copyleft spirit).
