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

## Module Structure
- `src/core/` — Types (AnalyzedTx, ScoredTx), pipeline, tx parsing
- `src/rpc/` — Bitcoin Core RPC client + ZMQ subscriber
- `src/db/` — SQLite UTXO cache (thread-safe via SharedDatabase)
- `src/signals/` — Scoring rules and composite score
- `src/ui/` — Dioxus desktop UI (feed, alerts, stats)
