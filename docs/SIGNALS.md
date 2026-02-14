# Signal Definitions & Scoring

## Philosophy

Not every whale move is a sell signal. The scoring system combines multiple metrics with weights to reduce false positives. Each signal is probabilistic, not binary.

## Scoring Rules

| Metric | Weight (0-10) | Description |
|--------|---------------|-------------|
| Tx Value (BTC) | 6 | Sum of outputs (economic value, excluding likely change) |
| UTXO Age | 8 | Time-since-last-move of inputs; old coins spending = narrative shock |
| Coin-Days Destroyed (CDD) | 9 | `sum(value_in * age_days)` — strongest combined signal |
| To-Exchange Probability | 10 | Output matches known exchange cluster/address |
| Dormant Cluster Activity | 7 | Cluster had no outgoing tx for extended period |
| Input Count | 4 | Many inputs → consolidation/wallet management |
| Fee Rate (sat/vB) | 3 | High urgency = potential hot path |
| Address Reuse / Script Migration | 3 | P2PK→SegWit etc. may indicate ownership change |
| Mempool Congestion Context | 3 | Congestion affects confirmation estimates |
| RBF Flag | 2 | Replaceable tx = signal may change |
| CPFP Characteristics | 2 | Fee-bumping = urgency indicator |
| Unbroadcast / Propagation | 2 | Seen but not widely propagated |
| Dust Consolidation | 1 | Usually operational, rarely bearish |
| CoinJoin Detection | -6 | If detected: reduce score (privacy tx, not directional) |

## Composite Score

```
score = sum(metric_value * weight) / max_possible_score * 100
```

Score is normalized to 0-100. Thresholds:

- **≥80**: 🔴 Critical — Likely market-moving
- **≥60**: 🟠 High — Worth watching
- **≥40**: 🟡 Medium — Notable but likely operational
- **<40**: ⚪ Low — Background noise

## False Positive Mitigation

Known patterns that inflate scores but are NOT directional signals:

1. **Internal cold storage shuffles** — Same cluster in/out
2. **UTXO consolidation** — Many small inputs, one large output back to same entity
3. **Batch payouts** — Exchange paying many users (many outputs, known sender)
4. **CoinJoin/mixing** — Equal-value outputs pattern
5. **Payment processor flows** — High volume but neutral

## Calibration

Weights are initial estimates. Must be calibrated via backtesting against:
- Price movement in 1h/4h/24h windows after signal
- Precision/Recall at each threshold
- Robustness across market regimes (bull/bear/range)
