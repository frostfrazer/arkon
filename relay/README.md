# ARKON Relay Server

Self-hostable relay for `arkon preview`. Zero external dependencies — single Go binary.

## Deploy in 60 seconds

```bash
# On any VPS (Ubuntu/Debian)
git clone https://github.com/arkon-sh/arkon
cd arkon/relay
go build -o arkon-relay .
./arkon-relay --port 8080 --base-url https://your-domain.com
```

With systemd:

```ini
# /etc/systemd/system/arkon-relay.service
[Unit]
Description=ARKON Relay Server
After=network.target

[Service]
ExecStart=/usr/local/bin/arkon-relay --port 8080 --base-url https://relay.yourdomain.com
Restart=always
User=www-data

[Install]
WantedBy=multi-user.target
```

```bash
systemctl enable --now arkon-relay
```

## Configure your clients

```toml
# arkon.toml
[targets.preview]
type = "webrtc"
ttl  = "24h"

# arkon.toml project-wide override
[project]
relay_url = "https://relay.yourdomain.com"
```

Or per-machine via environment variable:

```bash
export ARKON_RELAY_URL=https://relay.yourdomain.com
```

## API

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/register` | Register a peer |
| `DELETE` | `/register` | Deregister a peer |
| `GET` | `/p/:peer_id/*` | Proxy to peer |
| `GET` | `/health` | Health check + peer count |

The relay proxies HTTP traffic from browsers to the local ARKON preview server.
It holds no state beyond the peer registry (in-memory, TTL-based).
Expired registrations are swept every 60 seconds.

## Bandwidth

The relay forwards HTTP requests during the initial connection handshake.
Once libp2p DHT is used (arkon-p2p v2), the relay is only needed as a fallback
for peers behind symmetric NAT. Most connections will be direct.
