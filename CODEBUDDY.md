# CODEBUDDY.md

This file provides guidance to CodeBuddy Code when working with code in this repository.

## Project Overview

AI-powered ETF trading system for Chinese securities markets, implementing Al Brooks Price Action methodology. Connects to Futu OpenAPI (FutuOpenD) for market data and paper trading. Target instruments are Chinese ETFs and A-shares, following T+1 settlement rules.

## Tech Stack

- **Language:** Rust (edition 2021)
- **Build System:** Cargo workspace
- **Async Runtime:** Tokio
- **Decimal Arithmetic:** `rust_decimal` (never use `f64` for prices)
- **Broker Integration:** Futu OpenAPI via TCP/protobuf (`prost`)
- **Mutation Testing:** cargo-mutants

## Build & Run Commands

```bash
cargo build                          # Compile the project
cargo run --bin brooks-trader        # Run the CLI binary
cargo test                           # Run all tests
cargo test -p brooks-core            # Run tests for a single crate
cargo test bar::tests::test_bull_bar # Run a single test by name
cargo fmt                            # Format code
cargo clippy --all-targets           # Lint code
cargo mutants                        # Run mutation testing
```

## Workspace Structure

```
ai-brooks-etf-trading/
├── Cargo.toml                   # Workspace root
├── config/
│   └── default.toml             # Default configuration (Futu, strategy, backtest settings)
├── crates/
│   ├── core/                    # brooks-core: domain types (Bar, Order, Position, Signal, etc.)
│   ├── pa-engine/               # brooks-pa-engine: Brooks PA analysis engine
│   ├── market-data/             # brooks-market-data: Futu client, market data pipeline
│   ├── strategy/                # brooks-strategy: Strategy trait, BrooksStrategy, risk mgmt
│   ├── order-manager/           # brooks-order-manager: OMS, paper/Futu execution
│   ├── backtester/              # brooks-backtester: backtesting engine, metrics, reports
│   ├── china-market/            # brooks-china-market: T+1 settlement rules, sessions, calendar
│   └── app/                     # brooks-app: CLI binary (backtest, paper, fetch-data)
└── proto/                       # Futu OpenAPI .proto files (vendored)
```

## Crate Dependency Graph

```
brooks-app
├── brooks-strategy
│   ├── brooks-pa-engine
│   │   └── brooks-core
│   └── brooks-china-market
├── brooks-market-data
│   └── brooks-core, brooks-china-market
├── brooks-order-manager
│   └── brooks-core, brooks-china-market
├── brooks-backtester
│   └── brooks-core, brooks-pa-engine, brooks-strategy, brooks-order-manager, brooks-china-market
└── brooks-china-market
    └── brooks-core
```

## Key Architecture Decisions

- **`rust_decimal` for all financial calculations.** Never use `f64` for prices, PnL, or position sizing.
- **Trait-based abstractions.** `MarketDataProvider`, `OrderExecutor`, `Strategy`, `MarketRules`, `DataFeed` are all traits, enabling mocks for testing and swappable implementations.
- **Same `Strategy` trait for backtesting and live.** Only the `OrderExecutor` and `MarketDataProvider` differ between backtest and paper trading modes.
- **Multi-timeframe via separate `PriceActionAnalyzer` instances.** Each timeframe gets its own analyzer. The strategy coordinates them through `MultiTimeframeCoordinator`.
- **Chinese market rules isolated in `brooks-china-market`.** T+1 settlement for all instruments (ETFs and stocks), price limits, session hours (09:30-11:30, 13:00-15:00 CST), lunch break, and holiday calendar.
- **Futu OpenD via TCP/protobuf.** Uses `prost` + `prost-build` to compile vendored `.proto` files. FutuOpenD runs locally on `127.0.0.1:11111`.

## Core Domain Types (in `crates/core/src/`)

| File | Types | Purpose |
|------|-------|---------|
| `bar.rs` | `Bar` | OHLCV candle with body/tail/range analysis methods |
| `market.rs` | `SecurityId`, `Exchange`, `Direction`, `SecurityType` | Security identification and trade direction |
| `timeframe.rs` | `Timeframe` | Enum of supported timeframes (1min through weekly) |
| `signal.rs` | `Signal`, `SignalType`, `SignalContext` | Trading signals from Brooks PA analysis |
| `order.rs` | `Order`, `OrderType`, `OrderStatus`, `OrderId` | Order lifecycle management |
| `position.rs` | `Position` | Open position tracking with PnL calculations |
| `event.rs` | `MarketEvent`, `TradingEvent` | Event types for the event bus |

## Chinese Market Specifics (in `crates/china-market/src/`)

| File | Types | Purpose |
|------|-------|---------|
| `session.rs` | `TradingSession` | Market hours, lunch break detection |
| `rules.rs` | `MarketRules` trait, `ChinaMarketRules`, `PriceLimits` | T+1 settlement, price limits, tick size, lot size |
| `calendar.rs` | `TradingCalendar` | Holiday calendar, makeup trading days |
| `security_info.rs` | `SecurityInfo`, `Board` | ETF/stock metadata, board-specific rules |

## External Dependencies

- Futu OpenD must be running locally for paper trading and real-time data
- Configuration lives in `config/default.toml`
