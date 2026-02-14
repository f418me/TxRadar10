# Exchange Address Tagging

## Was ist das?

Exchange-Tagging bedeutet: Wir ordnen Bitcoin-Adressen bekannten Entitäten zu (Börsen, Services, Miner, etc.). Wenn eine Transaktion im Mempool auftaucht, die **an eine Exchange-Adresse sendet**, ist das potenziell ein **Verkaufssignal** — jemand bewegt Coins zur Börse, vermutlich um zu verkaufen.

Umgekehrt: Coins die **von einer Exchange wegbewegt** werden, deuten auf Akkumulation hin (Withdrawal → Cold Storage).

Deshalb hat dieses Signal Gewicht 10 — es ist das **kausal stärkste** Signal im ganzen System.

## Wie funktioniert es?

### 1. Adress-Datenbank

Eine lokale Datenbank mit bekannten Adressen:

```
Adresse → { entity: "Binance", type: "exchange", confidence: 0.95, source: "walletexplorer" }
```

### 2. Matching bei jeder Mempool-Tx

Für jede neue Transaktion im Mempool:

- **Outputs prüfen:** Geht ein Output an eine bekannte Exchange? → **"To-Exchange" Signal** (bearish)
- **Inputs prüfen:** Kommt ein Input von einer bekannten Exchange? → **"From-Exchange" Signal** (bullish/neutral)

### 3. Signal-Scoring

| Richtung | Interpretation | Score-Beitrag |
|----------|---------------|---------------|
| **→ Exchange** (Deposit) | Potentieller Verkaufsdruck | Hoch (positiv) |
| **← Exchange** (Withdrawal) | Akkumulation / Cold Storage | Negativ (reduziert Score) |
| **Exchange → Exchange** | Transfer, OTC, Arbitrage | Mittel (kontextabhängig) |
| **Intern (same entity)** | Wallet-Management | Ignorieren |

## Datenquellen

### Open Source / Frei verfügbar

| Quelle | Beschreibung | Format | Qualität |
|--------|-------------|--------|----------|
| **WalletExplorer.com** | Historisch grösste freie Datenbank. Cluster-basiert (Multi-Input-Heuristik). Labels für ~100+ Exchanges/Services. Website existiert noch, API eingeschränkt. | Web scraping / CSV exports | Mittel — veraltet, keine neuen Adressen seit ~2022 |
| **OXT.me** | Samourai-nahes Analyse-Tool. Cluster-Visualisierung, einige öffentliche Labels. | Web UI | Mittel |
| **Blockchain.com Explorer** | Zeigt Labels für grosse bekannte Adressen | Web | Niedrig — nur prominente Adressen |
| **BitcoinAbuse.com** | Bekannte Scam/Ransomware-Adressen | API/CSV | Nische — nicht Exchange-fokussiert |
| **GitHub: Verschiedene Repos** | Community-kuratierte Listen bekannter Adressen | CSV/JSON | Variabel — oft klein und veraltet |

### Kommerzielle Anbieter (für Referenz)

| Anbieter | Beschreibung | Preis |
|----------|-------------|-------|
| **Chainalysis** | Marktführer. Millionen getaggte Adressen, Echtzeit-Updates | Enterprise $$$ |
| **Elliptic** | Ähnlich wie Chainalysis | Enterprise $$$ |
| **Crystal Blockchain** | Gute API, kleinere Abdeckung | $$-$$$ |
| **Arkham Intelligence** | Neuerer Player, teilweise free tier | Freemium |
| **Glassnode** | On-chain Metriken, Exchange-Flow-Daten | Ab ~$40/Monat |

### Selbst erstellbar (Heuristiken)

| Methode | Beschreibung | Aufwand |
|---------|-------------|--------|
| **Multi-Input-Heuristik** | Wenn mehrere Inputs in einer Tx → gleicher Besitzer (Common-Input-Ownership). Cluster bilden. | Mittel |
| **Change-Detection** | Change-Output identifizieren → Cluster erweitern | Mittel |
| **Known Deposit Addresses** | Exchange-Einzahlungsadressen sammeln (eigene Accounts, öffentliche Berichte) | Manuell |
| **Behavioral Patterns** | Exchanges haben typische Muster: viele kleine Outputs (Batch-Payouts), hohe Frequenz | Hoch |

## Unser Ansatz für TxRadar10

### Phase 1: Statische Adresslisten (MVP)

1. **Seed-Adressen sammeln** — Bekannte Hot/Cold Wallets der grössten Exchanges:
   - Binance, Coinbase, Kraken, Bitfinex, Gemini, OKX, Bybit, etc.
   - Quellen: Blockchain Explorer Labels, öffentliche Berichte, Community-Listen
   
2. **SQLite-Tabelle:**
   ```sql
   CREATE TABLE address_tags (
       address     TEXT PRIMARY KEY,
       entity      TEXT NOT NULL,      -- "Binance", "Coinbase", etc.
       entity_type TEXT NOT NULL,      -- "exchange", "miner", "service", "mixer"
       confidence  REAL DEFAULT 0.5,   -- 0.0-1.0
       source      TEXT,               -- Woher das Tag kommt
       updated_at  TEXT
   );
   ```

3. **Matching:** Bei jeder Tx die Output-Adressen gegen diese Tabelle prüfen.

### Phase 2: Cluster-Erweiterung

1. **Multi-Input-Heuristik** — Wenn wir eine getaggte Adresse als Input sehen, taggen wir alle anderen Inputs der gleichen Tx mit demselben Entity-Label (mit niedrigerer Confidence).

2. **Change-Output-Tracking** — Wenn eine bekannte Exchange-Adresse sendet, ist der Change-Output vermutlich auch die Exchange → taggen.

### Phase 3: Externe APIs (optional)

- Arkham Intelligence API (Freemium) für Echtzeit-Labels
- Oder eigener Crawler für WalletExplorer/OXT

## Bekannte Probleme & Pitfalls

### False Positives
- **Deposit-Adressen werden recycled** — Alte Exchange-Adressen werden evtl. nicht mehr genutzt
- **Custodial Services** — Adressen die aussehen wie Exchanges sind manchmal Payment Processors
- **Cluster Collapse** — Multi-Input-Heuristik scheitert bei CoinJoin → ganzer Cluster wird falsch zugeordnet

### False Negatives
- **Neue Adressen** — Exchanges rotieren Adressen; statische Listen veralten schnell
- **Sub-Entities** — Grosse Exchanges haben hunderte Hot Wallets, wir kennen nicht alle
- **P2P / DEX** — Dezentrale Exchanges haben keine festen Adressen

### Confidence-Modell

Deshalb arbeiten wir mit `confidence` (0.0-1.0):
- **0.9-1.0:** Direkt vom Exchange bestätigt oder aus verlässlicher Quelle
- **0.7-0.9:** Aus Multi-Input-Clustering abgeleitet
- **0.5-0.7:** Community-Labels, ältere Daten
- **<0.5:** Heuristisch, unbestätigt

Der Score im Signal Engine wird mit der Confidence gewichtet:
```
exchange_signal = is_to_exchange * confidence * weight
```

## Adressen-Umfang (Grössenordnung)

- **Realistisch für MVP:** ~500-2000 bekannte Adressen der Top-20 Exchanges
- **Mit Clustering:** ~10'000-100'000 Adressen
- **Kommerzielle Anbieter:** Millionen getaggte Adressen

Für einen signifikanten Prozentsatz der Mempool-Txs zu matchen, brauchen wir mittelfristig Clustering. Aber selbst mit 1000 Adressen fangen wir die grössten Whale-Moves ab, weil die meisten über bekannte Hot Wallets laufen.
