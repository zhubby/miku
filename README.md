# Miku

Miku is an early-stage Rust Kubernetes management application inspired by Lens. It is designed as a multi-crate workspace with one service contract shared by:

- a native egui desktop app (`miku` / `miku gui`)
- a BS mode with a Rust server plus wasm egui UI (`miku server`)

## Architecture

The workspace keeps implementation details behind crate boundaries:

- `miku-core`: domain types, identifiers, Kubernetes resource references, and shared errors
- `miku-api`: service traits and DTO contracts shared by UI, server, clients, and implementations
- `miku-store`: `~/.miku` directory and SQLite-backed local persistence
- `miku-kube`: Kubernetes integration built on `kube-rs`
- `miku-server`: REST/JSON and streaming transport adapter over the service traits
- `miku-http-client`: client facade for wasm/BS mode
- `miku-ui`: egui application shell
- `miku-web`: wasm entrypoint for the web UI
- `miku-cli`: `clap` entrypoint with `gui` and `server` subcommands

## Local Data

By default, Miku stores user data under `~/.miku`:

- `miku.db` for SQLite data
- `config.toml` for small human-readable settings
- `logs/` for local logs
- `cache/` for disposable cache data

The application should not copy Kubernetes credentials into SQLite. It references existing kubeconfig contexts unless a future secret-store design changes that policy.

## Development

Native GUI:

```sh
cargo run -p miku-cli
```

Server mode:

```sh
cargo run -p miku-cli -- server --bind 127.0.0.1:5174
```

Web UI static assets:

```sh
make build-web-assets
```

The Makefile target builds `miku-web` for `wasm32-unknown-unknown`, runs `wasm-bindgen`
with `--target web`, and writes generated assets into
`crates/miku-server/web-dist`. Those generated files are embedded into
`miku-server` at compile time and are intentionally ignored by git. The
installed `wasm-bindgen-cli` version must match the resolved `wasm-bindgen`
crate version in `Cargo.lock`.

Web UI check target:

```sh
make check-web
```

Quality checks:

```sh
make fmt
make clippy
make test
```
