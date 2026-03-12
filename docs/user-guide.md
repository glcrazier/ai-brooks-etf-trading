# Brooks ETF Trading System - User Guide

## Overview

The Brooks ETF Trading System is an AI-powered trading tool for Chinese securities markets. It implements Al Brooks Price Action methodology to analyze market structure, identify trading signals, and execute trades on Chinese ETFs via the Futu OpenAPI.

Key capabilities:

- **Backtesting** - Test strategies against historical price data with detailed performance metrics.
- **Paper trading** - Run the strategy in real time with live market data (no real money).
- **Price action analysis** - Classify bars, detect trends, channels, and support/resistance levels.
- **Data fetching** - Download historical bars from Futu and save as CSV for offline use.

Target instruments are Chinese ETFs that support T+0 intraday trading (e.g., SSE 50 ETF `510050`, CSI 300 ETF `510300`).

---

## Prerequisites

1. **Rust toolchain** (1.70 or later)

   Install via [rustup](https://rustup.rs/):

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Futu OpenD** (required for paper trading and data fetching)

   Download from [Futu OpenD](https://openapi.futunn.com/futu-api-doc/en/intro/FutuOpenDGuide.html). FutuOpenD must be running locally before using the `paper` or `fetch-data` commands.

   Default connection: `127.0.0.1:11111`

> **Note:** Backtesting and analysis can run without FutuOpenD if you supply CSV data files.

---

## Installation

```bash
# Clone the repository
git clone https://github.com/glcrazier/ai-brooks-etf-trading.git
cd ai-brooks-etf-trading

# Build the project
cargo build --release

# Verify the build
./target/release/brooks-trader --help
```

The binary is `brooks-trader`. During development you can also use:

```bash
cargo run --bin brooks-trader -- --help
```

---

## Configuration

All settings live in a TOML file. The default is `config/default.toml`. You can specify a different file with the `--config` flag.

### Configuration sections

#### `[futu]` - Futu OpenD connection

| Key | Default | Description |
|-----|---------|-------------|
| `host` | `"127.0.0.1"` | FutuOpenD host address |
| `port` | `11111` | FutuOpenD TCP port |
| `client_id` | `"brooks-trading"` | Client identifier |

#### `[market]` - Market and instrument settings

| Key | Default | Description |
|-----|---------|-------------|
| `exchange` | `"SH"` | Exchange: `SH` (Shanghai) or `SZ` (Shenzhen) |
| `securities` | `["510050", "510300", "510500", "159915"]` | Security codes to trade |
| `primary_timeframe` | `"5min"` | Main trading timeframe |
| `context_timeframe` | `"daily"` | Higher timeframe for context analysis |

Supported timeframes: `1min`, `5min`, `15min`, `30min`, `60min`, `daily`, `weekly`

#### `[strategy]` - Strategy selection

| Key | Default | Description |
|-----|---------|-------------|
| `name` | `"brooks_pa"` | Strategy identifier |
| `warm_up_bars` | `200` | Number of bars to process before generating signals |

#### `[strategy.risk]` - Risk management

| Key | Default | Description |
|-----|---------|-------------|
| `max_risk_per_trade_pct` | `1.0` | Maximum risk per trade as % of capital |
| `max_daily_loss_pct` | `3.0` | Maximum daily loss as % of capital (stops trading when hit) |
| `max_open_positions` | `3` | Maximum simultaneous open positions |
| `initial_capital` | `100000.0` | Starting capital in yuan |

#### `[strategy.pa]` - Price action parameters

| Key | Default | Description |
|-----|---------|-------------|
| `ema_period` | `20` | EMA period for trend detection |
| `swing_lookback` | `10` | Lookback window for swing high/low detection |
| `doji_threshold` | `0.2` | Body-to-range ratio below which a bar is classified as a doji |
| `sr_cluster_tolerance` | `0.005` | Proximity tolerance (as fraction) for clustering support/resistance levels |

#### `[backtest]` - Backtesting settings

| Key | Default | Description |
|-----|---------|-------------|
| `start_date` | `"2024-01-01"` | Backtest period start (YYYY-MM-DD) |
| `end_date` | `"2024-12-31"` | Backtest period end (YYYY-MM-DD) |
| `fill_model` | `"next_bar_open"` | Order fill model |
| `slippage_bps` | `5` | Simulated slippage in basis points |

#### `[logging]` - Logging configuration

| Key | Default | Description |
|-----|---------|-------------|
| `level` | `"info"` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `format` | `"pretty"` | Output format: `pretty` or `json` |
| `file` | `"logs/trading.log"` | Optional log file path |

### Validating configuration

```bash
brooks-trader validate-config
# or with a custom config file:
brooks-trader --config my-config.toml validate-config
```

---

## CLI Reference

### Global flags

| Flag | Default | Description |
|------|---------|-------------|
| `--config <PATH>` | `config/default.toml` | Path to TOML configuration file |
| `--log-level <LEVEL>` | (from config) | Override log level |

---

### `backtest` - Run a historical backtest

Execute the Brooks Price Action strategy against historical data and report performance metrics.

```bash
brooks-trader backtest [OPTIONS]
```

**Options:**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--start-date <DATE>` | No | From config | Start date (YYYY-MM-DD) |
| `--end-date <DATE>` | No | From config | End date (YYYY-MM-DD) |
| `--securities <LIST>` | No | From config | Comma-separated security codes |
| `--capital <AMOUNT>` | No | From config | Initial capital in yuan |
| `--timeframe <TF>` | No | From config | Primary timeframe |
| `--data-file <PATH>` | No | -- | Load bars from a CSV file |
| `--output-file <PATH>` | No | -- | Save results to a JSON file |

**Examples:**

```bash
# Backtest using config defaults (requires FutuOpenD)
brooks-trader backtest

# Backtest from CSV data (no FutuOpenD needed)
brooks-trader backtest --data-file data/510050_5min.csv

# Custom date range and capital
brooks-trader backtest \
  --start-date 2024-06-01 \
  --end-date 2024-12-31 \
  --capital 200000 \
  --output-file results.json
```

---

### `paper` - Start paper trading

Connect to FutuOpenD and run the strategy with live market data. No real orders are placed.

```bash
brooks-trader paper [OPTIONS]
```

**Options:**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--securities <LIST>` | No | From config | Comma-separated security codes |
| `--capital <AMOUNT>` | No | From config | Initial capital in yuan |

**Example:**

```bash
brooks-trader paper --securities "510050,510300" --capital 100000
```

> **Requires:** FutuOpenD running at the configured host/port.

---

### `fetch-data` - Download historical data

Fetch historical bar data from FutuOpenD and save as CSV files.

```bash
brooks-trader fetch-data [OPTIONS]
```

**Options:**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--securities <LIST>` | No | From config | Comma-separated security codes |
| `--timeframe <TF>` | No | `5min` | Timeframe to fetch |
| `--start-date <DATE>` | Yes | -- | Start date (YYYY-MM-DD) |
| `--end-date <DATE>` | Yes | -- | End date (YYYY-MM-DD) |
| `--output-dir <DIR>` | No | `./data` | Output directory |

**Example:**

```bash
brooks-trader fetch-data \
  --securities "510050" \
  --timeframe 5min \
  --start-date 2024-01-01 \
  --end-date 2024-12-31 \
  --output-dir ./data
```

Output files are named `{security}_{timeframe}.csv` (e.g., `510050_5min.csv`).

> **Requires:** FutuOpenD running at the configured host/port.

---

### `analyze` - Run price action analysis

Analyze bar data using Brooks Price Action methodology and display the results.

```bash
brooks-trader analyze [OPTIONS]
```

**Options:**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--data-file <PATH>` | No | -- | Load bars from CSV file |
| `--security <CODE>` | No | From config | Security code |
| `--timeframe <TF>` | No | `5min` | Timeframe |
| `--format <FMT>` | No | `text` | Output format: `text` or `json` |

**Examples:**

```bash
# Text analysis
brooks-trader analyze --data-file data/510050_5min.csv --format text

# JSON output for programmatic use
brooks-trader analyze --data-file data/510050_5min.csv --format json
```

---

### `validate-config` - Validate configuration

Check the configuration file for errors without running any trading logic.

```bash
brooks-trader validate-config
```

Exits with code 0 on success, non-zero if validation errors are found.

---

### `show-config` - Display configuration

Print the loaded configuration values.

```bash
brooks-trader show-config [OPTIONS]
```

**Options:**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--section <NAME>` | No | (all) | Show only a specific section |

Valid sections: `futu`, `market`, `strategy`, `backtest`, `logging`

**Examples:**

```bash
# Show all settings
brooks-trader show-config

# Show only risk management settings
brooks-trader show-config --section strategy
```

---

## Workflows

### Workflow 1: Backtest with historical CSV data

```bash
# Step 1: Fetch historical data (requires FutuOpenD)
brooks-trader fetch-data \
  --securities "510050" \
  --timeframe 5min \
  --start-date 2024-01-01 \
  --end-date 2024-12-31

# Step 2: Run the backtest
brooks-trader backtest \
  --data-file data/510050_5min.csv \
  --output-file backtest_results.json

# Step 3: Review results
cat backtest_results.json
```

### Workflow 2: Live paper trading session

```bash
# Step 1: Verify your configuration
brooks-trader validate-config

# Step 2: Review settings
brooks-trader show-config --section strategy

# Step 3: Start FutuOpenD (external step)
# Ensure FutuOpenD is running on 127.0.0.1:11111

# Step 4: Start paper trading
brooks-trader paper --securities "510050,510300" --capital 100000
```

### Workflow 3: Price action analysis

```bash
# Analyze existing data for trading signals and market structure
brooks-trader analyze \
  --data-file data/510050_5min.csv \
  --security "510050" \
  --format text
```

The analysis output includes:
- Current trend state (bull/bear/trading range)
- Bar classifications (trend bars, reversals, dojis, inside/outside bars)
- Support and resistance levels
- Active trading signals

---

## CSV Data Format

CSV files used by `--data-file` and produced by `fetch-data` follow this format:

```
timestamp,open,high,low,close,volume
2024-01-02T09:35:00+08:00,2.345,2.350,2.340,2.348,150000
2024-01-02T09:40:00+08:00,2.348,2.355,2.346,2.352,120000
```

| Column | Type | Description |
|--------|------|-------------|
| `timestamp` | ISO 8601 datetime | Bar timestamp (with timezone) |
| `open` | Decimal | Opening price |
| `high` | Decimal | Highest price in the bar |
| `low` | Decimal | Lowest price in the bar |
| `close` | Decimal | Closing price |
| `volume` | Integer | Trading volume |

---

## Troubleshooting

### FutuOpenD connection refused

```
Error: Failed to connect to FutuOpenD at 127.0.0.1:11111
```

- Verify FutuOpenD is running: check the FutuOpenD application or its system tray icon.
- Confirm the host and port in `config/default.toml` under `[futu]` match your FutuOpenD settings.

### Configuration validation errors

```
Error: Configuration validation failed
```

Run `brooks-trader validate-config` to see specific errors. Common issues:
- `exchange` must be `SH` or `SZ`
- `securities` list must not be empty
- Risk parameters (`max_risk_per_trade_pct`, `max_daily_loss_pct`) must be positive
- Date formats must be `YYYY-MM-DD`

### Lot size errors in backtesting

```
Error: quantity 150 is not a multiple of lot size 100
```

Chinese ETFs trade in lots of 100 shares. Adjust `initial_capital` or `max_risk_per_trade_pct` so that position sizing produces multiples of 100.

### No trading signals generated

If the backtest produces zero trades:
- Increase the data range. The strategy requires `warm_up_bars` (default: 200) bars before generating any signals.
- Check that the data file contains enough bars for the warm-up period.
- Use `debug` log level (`--log-level debug`) to see why signals are being filtered.

### Market session issues

Chinese market hours are:
- Morning session: 09:30 - 11:30 CST
- Afternoon session: 13:00 - 15:00 CST

The strategy only generates entry signals during active market hours and avoids entries near session open/close boundaries and the lunch break.
