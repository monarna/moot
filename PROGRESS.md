# Moot Progress

## Done
- Hollow Comments (w/ 2000 char limit, Steam-style profile comments)
- libp2p Swarm Integration (Kademlia + Gossipsub + Identify + Ping + Noise/Yamux)
- Channel bridge: `publish_tx` / `msg_tx`, HTTP P2P endpoints return 410 Gone
- Builds with **0 errors, 0 warnings** on stable Rust

### Tor Integration
- Removed `arti-client` / `tor-hsservice` deps (unstable API, nightly required)
- **Approach**: bundle static `tor` binary in release; spawn as subprocess via `TorManager`
- `TorManager::start()` now takes `tor_binary: &str` and `http_port: u16`
- Checks for bundled `tor.exe`/`tor` next to binary first, falls back to PATH
- `HiddenServicePort` dynamically matches `--port` value

### SOCKS-aware libp2p Transport
- `Socks5Transport` wraps libp2p TCP dialing through Tor SOCKS5 proxy (port 19050)
- Combined with TCP transport via `.or_transport()` → SOCKS preferred, TCP fallback
- `P2PNetwork::new()` accepts `socks_port: Option<u16>`

### Cross-platform (2026-05-11)
- **Default data dir**: `%APPDATA%\moot` on Windows, `/tmp/moot_data` on Linux
- **Path handling**: All file paths use `PathBuf::join()` — no hardcoded `/`
- **Database::open()**: accepts `impl AsRef<Path>` instead of `&str`
- **Tor binary**: `find_tor_binary()` discovers bundled `tor.exe` next to binary
- **Scripts**: `run.ps1` / `stop.ps1` for Windows, `run.sh` / `stop.sh` for Linux
- **`install-windows.ps1`**: Downloads Tor Expert Bundle, builds moot, creates data dirs
- **`.gitignore`**: ignores `target/`, `data/`, `*.pid`, `*.log`
- **`static/uploads/.gitkeep`**: ensures upload directory exists after clone

### Features Added
- **`--torrc-extra`**: Append custom lines to generated torrc
- **`/health` endpoint**: Returns Tor status and `.onion` address alongside project status

### Bug Fixes
- Fixed double `.onion` in Tor startup log line
- Removed `#[allow(dead_code)]` where no longer needed

## Key Decisions
- Single branch with `#[cfg(target_os = "...")]` guards — no platform branches
- Bundle static `tor` binary in release packaging
- SOCKS5 transport for outbound libp2p + regular TCP transport as fallback
- Users never need Rust — distribute compiled binary + tor.exe as ZIP

## Next Steps
1. Populate `BOOTSTRAP_NODES` with real `.onion` addresses
2. Bundle tor binary fetch in CI or `build.rs`
3. Revisit arti once API stabilizes (arti 1.0+)

## Critical Context
- `TorManager::start(data_dir, extra_config, tor_binary, http_port)` in `src/tor.rs`
- `Socks5Transport` in `p2p/network.rs` — `impl Transport` with output `Libp2pTcpStream`
- `#[cfg]` guards for `target_os = "windows"` in `default_data_dir()`, `tor_binary_name()`
- `install-windows.ps1` downloads Tor from `dist.torproject.org`, version 14.0.9
