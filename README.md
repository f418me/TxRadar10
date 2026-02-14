# TxRadar10 🟠

Real-time Bitcoin mempool monitor that detects whale movements, old-coin spending, and exchange flows — distilled into actionable trading signals.

## Architecture

```
┌─────────────┐     ZMQ      ┌──────────────┐     Score     ┌──────────────┐
│ Bitcoin Core │────────────▶│  Ingestion    │────────────▶│  Signal       │
│  (Full Node) │  sequence    │  Engine       │              │  Engine       │
└─────────────┘  rawtx       └──────────────┘              └──────┬───────┘
                                    │                              │
                                    ▼                              ▼
                             ┌──────────────┐              ┌──────────────┐
                             │  UTXO Cache   │              │  Dioxus UI   │
                             │  (SQLite)     │              │  (Desktop)   │
                             └──────────────┘              └──────────────┘
```

### Data Flow

1. **ZMQ Subscriber** — Listens to `sequence` (mempool add/remove/block events) and `rawtx` topics from local Bitcoin Core
2. **Tx Parser** — Decodes raw transactions, resolves prevouts via RPC + local cache
3. **UTXO Cache** — SQLite store for funding metadata (block height, timestamp, value, script type)
4. **Signal Engine** — Scores each transaction against configurable rules (see [SIGNALS.md](docs/SIGNALS.md))
5. **UI** — Dioxus desktop app showing live feed, signal alerts, and mempool stats

### Latency Target

- **Hot path** (ZMQ → Score): <1s for basic signals
- **Cold path** (cluster/tag enrichment): async, <30s

## Prerequisites

- Bitcoin Core with ZMQ enabled (see [SETUP.md](docs/SETUP.md))
- Rust 1.80+
- macOS / Linux

## Quick Start

```bash
# Ensure Bitcoin Core is running with ZMQ
bitcoin-cli getzmqnotifications

# Build & run
cargo run --release
```

## Project Structure

```
src/
├── main.rs          # Entry point, runtime setup
├── core/
│   ├── mod.rs       # Core types (MempoolEvent, ScoredTx, etc.)
│   ├── tx.rs        # Transaction model & parsing
│   └── mempool.rs   # Mempool state machine (Added/Replaced/Confirmed)
├── rpc/
│   ├── mod.rs       # Bitcoin Core RPC client
│   └── zmq.rs       # ZMQ subscriber (sequence, rawtx)
├── signals/
│   ├── mod.rs       # Signal engine orchestrator
│   ├── rules.rs     # Individual scoring rules
│   └── score.rs     # Composite score calculation
├── db/
│   ├── mod.rs       # SQLite UTXO cache & signal history
│   └── schema.rs    # DB schema & migrations
└── ui/
    ├── mod.rs       # Dioxus app root
    ├── feed.rs      # Live transaction feed
    ├── alerts.rs    # Signal alert panel
    └── stats.rs     # Mempool statistics dashboard
```

## Documentation

- [SIGNALS.md](docs/SIGNALS.md) — Signal definitions, weights, and scoring logic
- [SETUP.md](docs/SETUP.md) — Bitcoin Core configuration for TxRadar10
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — Detailed design decisions

## License

MIT
