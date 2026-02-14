# Architecture

## Design Principles

1. **Event-driven** — ZMQ events drive the pipeline, no polling
2. **Hot/Cold path split** — Basic scoring <1s, enrichment async <30s
3. **Mempool as state machine** — Tx lifecycle: `Pending → Confirmed | Replaced | Evicted`
4. **Probabilistic labels** — No binary whale/not-whale; confidence scores throughout
5. **Local-first** — All data stays local; no external API dependencies for core functionality

## Component Design

### ZMQ Subscriber (`rpc/zmq.rs`)

- Subscribes to `sequence` topic for ordered mempool events
- Subscribes to `rawtx` for full transaction data
- Detects missed events via sequence numbers → triggers resync
- Feeds `MempoolEvent` into async channel

### Mempool State (`core/mempool.rs`)

- In-memory map of pending transactions
- State transitions: Added → Replaced (RBF) / Removed (eviction) / Confirmed (mined)
- Tracks replacement chains for RBF analysis
- Exposes mempool statistics (size, fee histogram, age distribution)

### Prevout Resolution (`rpc/mod.rs`)

- On new tx: resolve each input's funding output
- Strategy: 1) Check local SQLite cache → 2) `getrawtransaction` RPC → 3) Mark as unresolved
- Cache hit = <1ms, RPC call = ~5-50ms
- Unresolved prevouts (pruned blocks, no txindex) get async retry

### Signal Engine (`signals/`)

- Rule-based scoring with configurable weights
- Each rule is a pure function: `fn(tx: &AnalyzedTx, ctx: &MempoolContext) -> RuleResult`
- Composite score aggregation with normalization
- Future: ML-based scoring as alternative engine

### UTXO Cache (`db/`)

- SQLite with WAL mode for concurrent read/write
- Schema: `utxo_cache(txid, vout, value, script_type, block_height, block_time)`
- Schema: `signals(id, txid, score, timestamp, rule_scores_json)`
- Periodic cleanup of spent/old entries

### UI (`ui/`)

- Dioxus desktop app
- Live feed: scrolling list of scored transactions
- Alert panel: filtered view for high-score signals
- Stats dashboard: mempool size, fee rates, CDD chart
- Signal detail view: breakdown of individual rule scores

## Data Flow

```
ZMQ(sequence) ──┐
                 ├──▶ MempoolEvent channel ──▶ State Machine ──▶ Signal Engine ──▶ UI update
ZMQ(rawtx) ─────┘         │                       │                   │
                           ▼                       ▼                   ▼
                     Parse & decode         Prevout resolve       Score & alert
                                            (cache + RPC)        (hot path <1s)
                                                 │
                                                 ▼
                                           SQLite cache
                                           (cold path)
```

## Threading Model

- **Main thread**: Dioxus UI event loop
- **Tokio runtime**: ZMQ subscriber, RPC calls, signal engine, DB writes
- Communication via `tokio::sync::broadcast` channels for UI updates
