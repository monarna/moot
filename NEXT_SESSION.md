# Next Session

## Critical Context
- **Tor integration**: `TorManager::start(data_dir, extra_config, tor_binary, http_port)` spawns system `tor` as subprocess. Signature has 4 params — check `src/tor.rs`.
- **SOCKS transport**: `Socks5Transport` in `p2p/network.rs` routes libp2p through Tor SOCKS when available. Combined with TCP via `Transport::or_transport()`.
- **CLI**: `--gateway`, `--data-dir`, `--port`, `--torrc-extra` flags via `clap`.
- **Cross-platform**: All paths use `PathBuf::join()`. `Database::open()` accepts `impl AsRef<Path>`. Windows data dir defaults to `%APPDATA%\moot`. Bundled `tor.exe` discovery in `find_tor_binary()`.
- **Scripts**: `run.sh`/`stop.sh` for Linux, `run.ps1`/`stop.ps1` for Windows, `install-windows.ps1` for Windows setup.
- **Peer persistence**: Peers saved to Sled every 60s, loaded on startup. `database::save_peers()` / `load_peers()` under `meta:peers` key.
- **Bootstrap nodes**: Empty `BOOTSTRAP_NODES` constant in `p2p/network.rs` — needs real addresses.
- **Health**: `/health` returns Tor status and `.onion` address via `web::Data<HealthState>`.
- **Build**: `edition = "2024"` on stable Rust. 0 errors, 0 warnings. Deps: `tokio-socks`.

## Remaining Work (Priority Order)
1. **Populate `BOOTSTRAP_NODES`** — Add the seed node's real `.onion` address to `p2p/network.rs`. The seed node will be on this Linux PC.
2. **Bundle tor binary** — `build.rs` or CI step to fetch static tor binary for release. Cross-compile Windows binary from Linux with `x86_64-pc-windows-gnu` target (needs `mingw-w64`).
3. **Create dist ZIP script** — `dist-windows.ps1` to package `moot.exe` + `tor.exe` for end-user distribution.
4. **Revisit arti** — Once `arti` 1.0+ stabilizes with a proper library API, consider switching back to in-process Tor.

## Cross-compilation for Windows (from this Linux PC)
```bash
sudo apt-get install mingw-w64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# Binary at: target/x86_64-pc-windows-gnu/release/moot.exe
```

## File Map
- `src/tor.rs` — `TorManager`, torrc generation, subprocess lifecycle
- `src/p2p/network.rs` — `Socks5Transport`, `BOOTSTRAP_NODES`, `P2PNetwork::new(socks_port: Option<u16>)`
- `src/main.rs` — CLI parsing, `default_data_dir()`, `find_tor_binary()`, Tor startup, health state
- `src/database.rs` — `open(impl AsRef<Path>)`, `save_peers()`, `load_peers()`
- `install-windows.ps1` — One-click Windows setup (downloads tor, builds, creates data dirs)
- `moot.service` — Systemd unit (Linux only)
- `run.sh` / `stop.sh` — Linux scripts (portable, use `$(dirname "$0")`)
- `run.ps1` / `stop.ps1` — Windows PowerShell scripts
