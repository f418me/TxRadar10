# TxRadar10 Architecture

## Overview
Real-time Bitcoin mempool monitor that scores transactions based on whale activity, UTXO age, and coin-days-destroyed signals.

## Data Flow

```
ZMQ (rawtx/hashblock)
    │
    ▼
┌──────────┐     ┌──────────────────────────────┐
│ zmq_sub  │────▶│         Pipeline              │
└──────────┘     │                                │
                 │  1. Parse raw tx               │
                 │  2. Resolve prevouts ◄──────┐  │
                 │  3. Calculate fee/fee_rate   │  │
                 │  4. Calculate CDD            │  │
                 │  5. Score via SignalEngine    │  │
                 │                              │  │
                 └──────────┬───────────────────┘  │
                            │                      │
                            ▼                      │
                 ┌──────────────────┐              │
                 │   Dioxus UI      │              │
                 │  (feed/alerts)   │              │
                 └──────────────────┘              │
                                                   │
              ┌────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│       Prevout Resolution            │
│                                     │
│  1. SQLite Cache (data/utxo_cache)  │
│     └─ hit? → return immediately    │
│                                     │
│  2. Bitcoin Core RPC                │
│     └─ getrawtransaction(txid,true) │
│     └─ extract vout[n].value        │
│     └─ extract blocktime/height     │
│     └─ cache result in SQLite       │
│                                     │
│  3. Unresolved (graceful degrade)   │
│     └─ score with available data    │
│     └─ prevouts_resolved = false    │
└─────────────────────────────────────┘
```

## Prevout Resolution Details

For each transaction input, we need the **funding transaction** to determine:
- **Value**: `getrawtransaction(prev_txid).vout[prev_vout].value`
- **Block time**: When the funding tx was confirmed (for UTXO age)
- **Block height**: For oldest_input_height tracking

### Computed Fields
- `total_input_value` = sum of all resolved prevout values
- `fee` = total_input_value - total_output_value
- `fee_rate` = fee / vsize (sat/vB)
- `coin_days_destroyed` = Σ(input_value_btc × age_days)
- `oldest_input_time` = min(block_time) across all inputs
- `oldest_input_height` = min(block_height) across all inputs

### Graceful Degradation
During IBD or for pruned blocks, some prevouts cannot be resolved. The pipeline:
- Marks `prevouts_resolved = false` if any input is unresolved
- Scores with whatever data is available (unresolved inputs contribute 0)
- CDD/age signals return 0.0 when data is missing

## RPC Configuration
Credentials are loaded in order:
1. Environment variables (`BITCOIN_RPC_HOST`, `BITCOIN_RPC_PORT`)
2. Cookie auth (`~/Library/Application Support/Bitcoin/.cookie`)
3. `bitcoin.conf` (`rpcuser`/`rpcpassword`)
4. Defaults (bitcoinrpc:bitcoinrpc @ 127.0.0.1:8332)

## Mempool State Tracking

`MempoolState` (in `src/core/mempool.rs`) tracks the lifecycle of every transaction:

- **Pending → Confirmed**: Tx included in a block (via `TxRemoved` with `Confirmed` reason)
- **Pending → Replaced**: Tx replaced by RBF (via `TxRemoved` with `Replaced` reason)
- **Pending → Evicted**: Tx evicted from mempool (size limit, conflict, etc.)

### Statistics exposed:
- `pending_count()` — number of unconfirmed txs
- `total_fees()` — sum of fees of all pending txs (sats)
- `total_vsize()` — sum of vsize of all pending txs
- `fee_histogram()` — distribution across buckets: 1-5, 5-10, 10-20, 20-50, 50-100, 100+ sat/vB

### RBF Replacement Chains
When a tx is replaced, the `replaced_by` field records the replacing txid. This enables
tracking multi-hop RBF chains. Requires the ZMQ `sequence` topic (TODO).

### Pruning
Confirmed/evicted entries are retained for 5 minutes (for UI display), then pruned.

### Stats Updates
Stats are sent to the UI every 100 txs or every 5 seconds (whichever comes first),
to avoid overwhelming the UI with per-tx updates.

## Module Structure
- `src/core/` — Types (AnalyzedTx, ScoredTx), pipeline, tx parsing, mempool state
- `src/rpc/` — Bitcoin Core RPC client + ZMQ subscriber
- `src/db/` — SQLite UTXO cache (thread-safe via SharedDatabase)
- `src/signals/` — Scoring rules and composite score
- `src/ui/` — Dioxus desktop UI (feed, alerts, stats with fee histogram)
