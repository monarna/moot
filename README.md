# Moot - Decentralized Social Network

A decentralized social network with tree-themed architecture, sway-weighted moderation, Tor hidden services, and libp2p P2P networking.

## Features

### Core Concepts
- **Roots** - Finite community boards (: /a\, /b\, /c\, and so on etc.)
- **Branches** - Sub-boards within roots (like sub-forums, generals, persistent long-running threads)
- **Leaves** - Posts/threads within branches
- **Hollows** - Private user spaces (like personal profiles)
- **Sway** - Normalized reputation system based on hosting contributions with a half-life
- **Moderation** - Sway-weighted community-run blacklisting, no powertripping Reddit mods, no corporate moderation. 

### Technical Features
- **Tor Hidden Service** - Automatic `.onion` address via system `tor` subprocess
- **P2P over Tor** - All libp2p traffic routes through SOCKS5 proxy when Tor is active
- **Gateway mode** - `--gateway` flag to run without Tor
- **libp2p Swarm** - Kademlia DHT + Gossipsub + Identify + Noise encryption
- **Sled Database** - Embedded database, no external DB needed
- **REST API** - Actix-web backend
- **WebSocket P2P Sync** - Real-time frontend updates via `/api/p2p/ws`
- **Cross-platform** - Linux and Windows (single codebase, `#[cfg]` guards)
- **Hollow Comments** - Public profile comment walls (Steam-style)
- **Content Lifecycle** - TTL-based seeding, background expiry, legendary promotion

## Architecture

```
Moot/
├── src/
│   ├── main.rs              # CLI, server, routes, cross-platform helpers
│   ├── models.rs            # Data structures
│   ├── database.rs          # Sled database operations
│   ├── crypto.rs            # Garlic routing, encryption, sanitization
│   ├── tor.rs               # Tor subprocess manager
│   └── p2p/
│       ├── mod.rs
│       └── network.rs       # libp2p swarm, Socks5Transport
├── static/
│   ├── index.html           # Frontend
│   └── uploads/             # Uploaded images
├── install-windows.ps1      # One-click Windows installer
├── run.sh / stop.sh         # Linux scripts
├── run.ps1 / stop.ps1       # Windows PowerShell scripts
└── moot.service             # Systemd unit
```

## Quick Start

### Linux
```bash
# Prerequisites: Rust 1.85+
# Optional: tor (for hidden service)
#   sudo apt-get install tor

git clone <repo-url>
cd Moot
cargo build --release
./target/release/moot
# Runs at http://127.0.0.1:8080
```

### Windows (one-time setup)
```powershell
# 1. Install Rust (one minute):
#    Download from https://rustup.rs or:
winget install Rustlang.Rustup

# 2. Build and install:
cd Moot
.\install-windows.ps1

# 3. Run:
.\run.ps1
```

### CLI Options
```bash
moot                    # Full mode (Tor + P2P)
moot --gateway          # No Tor, plain HTTP/P2P
moot --port 9090        # Custom port
moot --data-dir ./data  # Custom data directory
moot --torrc-extra "ExitNodes {us}"  # Extra Tor config
```

## Distributing to Users

### Linux
Archive `target/release/moot` as a single binary. Users just run `./moot`.

### Windows
Run `install-windows.ps1` once on a machine with Rust. It produces:
- `target/release/moot.exe`
- `target/release/tor.exe` (downloaded by the script)

Zip these two files together. Users unzip and double-click `moot.exe`.

### iOS
Not feasible — iOS apps can't run background processes or listen on TCP ports. Use the web frontend with a gateway node.

## API Endpoints

### Roots
- `GET /api/roots` - List all roots
- `GET /api/root/{id}` - Get specific root

### Branches
- `POST /api/branch/{root_id}` - Create branch
- `GET /api/branches/{root_id}` - List branches in root
- `GET /api/branch/{branch_id}` - Get specific branch

### Leaves (Posts)
- `POST /api/leaf/{address}` - Create leaf
- `GET /api/leaves/{root_id}` - List leaves in root
- `POST /api/upvote_leaf/{leaf_id}/{address}` - Upvote
- `POST /api/downvote_leaf/{leaf_id}/{address}` - Downvote
- `POST /api/mirror_leaf/{leaf_id}/{address}` - Mirror
- `GET /api/mirrored_leaves/{address}` - Get mirrored leaves

### Hollows (Private Spaces)
- `POST /api/hollow/{address}` - Create hollow
- `GET /api/hollow/{address}` - Get hollow info
- `POST /api/hollow/{address}/settings` - Update settings
- `POST /api/hollow/{address}/post` - Add post
- `DELETE /api/hollow/{address}/post/{post_id}` - Delete post
- `POST /api/hollow/{address}/comment` - Add comment
- `GET /api/hollow/{address}/comments` - List comments
- `DELETE /api/hollow/{address}/comment/{comment_id}` - Delete comment

### Sway & Moderation
- `GET /api/sway/{address}` - Get user's sway
- `POST /api/sway/report/{address}` - Report hosting stats
- `POST /api/report` - Report content/user
- `POST /api/vote_blacklist` - Vote to blacklist
- `POST /api/vote_dismiss` - Vote to dismiss
- `GET /api/blacklist/{target_type}/{target_id}` - Check blacklist status

### Content Lifecycle
- `POST /api/legendary/promote/{leaf_id}?address=` - Promote to legendary
- `POST /api/legendary/remove/{leaf_id}?address=` - Remove from legendary
- `GET /api/legendary?address=` - List legendary entries
- `GET /api/leaf/{id}/expiry` - Check leaf expiry
- `GET /api/node/config` - Get node config
- `POST /api/node/config` - Update node config

### P2P
- `GET /api/p2p/ws` - WebSocket for real-time frontend updates
- `POST /api/peers/add` - Add peer by multiaddr
- `GET /api/peers/list` - List connected peers

### Health
- `GET /health` - Health check (returns Tor status + .onion address)

## Technology Stack

- **Backend**: Rust, Actix-web, Tokio
- **Database**: Sled (embedded)
- **P2P**: libp2p (Kademlia, Gossipsub, Identify, Ping, Noise/Yamux)
- **Tor**: System `tor` subprocess, SOCKS5 via `tokio-socks`
- **Frontend**: Vanilla JS, PWA
- **Crypto**: Ed25519, X25519, SHA-256

## Design Philosophy

### "Equal Ownership"
- This is system for the community, driven by it, owned by it. There is no Mark Zuckerberg, there is no server, no datacenter. It is hosted by its users. 
- Node runners are gods — hosting earns sway (reputation)
- Browsers lurk — casual users have limited powers
- Sway from hosting/posting — more bandwidth/hours, more valuable posts = more moderation power

## License

MIT
