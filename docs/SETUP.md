# Bitcoin Core Setup for TxRadar10

## Required Configuration

TxRadar10 connects to a local Bitcoin Core node via ZMQ and RPC.

### bitcoin.conf

```ini
# Server mode
server=1

# Pruning (full chain not needed)
prune=550

# Large mempool for full visibility
maxmempool=1000
mempoolexpiry=336

# RPC access
rpcuser=bitcoinrpc
rpcpassword=<generated>
rpcallowip=127.0.0.1
rpcbind=127.0.0.1
rpcport=8332

# ZMQ — required for TxRadar10
zmqpubrawblock=tcp://127.0.0.1:28332
zmqpubrawtx=tcp://127.0.0.1:28333
zmqpubhashtx=tcp://127.0.0.1:28334
zmqpubhashblock=tcp://127.0.0.1:28335
```

### ZMQ Topics Used

| Topic | Port | Purpose |
|-------|------|---------|
| `rawtx` | 28333 | Raw transaction bytes on mempool entry |
| `hashtx` | 28334 | Txid notification (lightweight) |
| `hashblock` | 28335 | New block notification |
| `sequence`* | — | Mempool lifecycle events (add/remove/block) |

*`sequence` topic requires `zmqpubsequence=tcp://127.0.0.1:28336` — add this to config.

### Verify ZMQ

```bash
bitcoin-cli getzmqnotifications
```

Should list all configured endpoints.

### Notes

- **Pruning**: We don't need full blockchain, but prevout resolution for very old UTXOs may require RPC calls that fail for pruned blocks. Mitigation: cache UTXO metadata in local SQLite.
- **txindex**: Not required with pruning. We resolve prevouts via `getrawtransaction` with blockhash hint or cache.
- **Security**: ZMQ has no authentication. Bind only to localhost.
