# Repository Guidelines

<!-- BEGIN COMPOUND CODEX TOOL MAP -->
## Compound Codex Tool Mapping (Claude Compatibility)

This section maps Claude Code plugin tool references to Codex behavior.
Only this block is managed automatically.

Tool mapping:
- Read: use shell reads (cat/sed) or rg
- Write: create files via apply_patch
- Edit/MultiEdit: use apply_patch
- Bash: use shell_command
- Grep: use rg (fallback: grep)
- Glob: use rg --files or find
- LS: use ls via shell_command
- WebFetch/WebSearch: use curl or Context7 for library docs
- AskUserQuestion/Question: present choices as a numbered list in chat and wait for a reply number. For multi-select (multiSelect: true), accept comma-separated numbers. Never skip or auto-configure -- always wait for the user's response before proceeding.
- Task/Subagent/Parallel: run sequentially in main thread; use multi_tool_use.parallel for tool calls
- TodoWrite/TodoRead: use file-based todos in todos/ with todo-create skill
- Skill: open the referenced SKILL.md and follow it
- ExitPlanMode: ignore
<!-- END COMPOUND CODEX TOOL MAP -->

@/Users/zhubby/.codex/RTK.md

## Project Structure & Module Organization

This repository is an early-stage Rust workspace for Miku, a Kubernetes management application inspired by Lens. Crates are split by responsibility:

- `crates/miku-core`: domain types, identifiers, Kubernetes resource references, shared paths, and shared errors.
- `crates/miku-api`: service traits, DTOs, and runtime-neutral contracts shared by UI, server, clients, and implementations.
- `crates/miku-store`: `~/.miku` directory management and SQLite-backed local persistence.
- `crates/miku-kube`: Kubernetes integration built on `kube-rs`, including resource listing and pod log access.
- `crates/miku-server`: Axum REST/JSON and streaming transport adapter over `miku-api` traits.
- `crates/miku-http-client`: HTTP client facade for web/BS mode.
- `crates/miku-ui`: shared egui application shell used by native and web runtimes.
- `crates/miku-web`: wasm entrypoint for the web UI.
- `crates/miku-cli`: `clap` entrypoint with `gui` and `server` subcommands.

Keep domain contracts in `miku-core`/`miku-api`, persistence details in `miku-store`, Kubernetes details in `miku-kube`, transport details in `miku-server`/`miku-http-client`, and UI state/rendering in `miku-ui`. Avoid leaking CLI-specific or runtime-specific concerns into the core crates.

## Build, Test, and Development Commands

Run commands from the repository root. Per the RTK instructions, prefix shell commands with `rtk` when working as an agent:

- `rtk cargo check --workspace`: fast compile verification.
- `rtk cargo build --workspace`: build all crates.
- `rtk cargo test --workspace`: run unit and integration tests.
- `rtk cargo fmt --all -- --check`: verify formatting.
- `rtk cargo fmt --all`: apply Rust formatting.
- `rtk cargo clippy --workspace --all-targets -- -D warnings`: lint strictly.
- `rtk cargo run -p miku-cli`: run the native GUI (default command).
- `rtk cargo run -p miku-cli -- gui`: run the native GUI explicitly.
- `rtk cargo run -p miku-cli -- server --bind 127.0.0.1:5174`: run server mode.
- `rtk cargo build -p miku-web --target wasm32-unknown-unknown`: build the wasm UI target.

The checked-in toolchain uses stable Rust with `clippy`, `rustfmt`, and `wasm32-unknown-unknown`.

## Rust Style and Idioms

- Target Rust 2024 for new code and examples.
- Follow `rustfmt` output with `max_width = 100`, field init shorthand, and try shorthand.
- Use concrete `struct`/`enum` types over `serde_json::Value` wherever the shape is known. Keep raw JSON at API/resource boundaries where Kubernetes objects are intentionally dynamic.
- Match on types, not display strings. Convert to strings at serialization, logging, and display boundaries.
- Prefer `From`/`Into`/`TryFrom`/`TryInto` for conversions when they clarify ownership and validation.
- Use traits for behavior boundaries. Keep service contracts in `miku-api` and concrete implementations in adapter crates.
- Prefer async service boundaries that work in both native and wasm contexts. Respect the existing `ServiceBounds` and `async_trait(?Send)` split for `wasm32`.
- Run independent async work concurrently when it is actually independent (`tokio::join!`, `futures::join_all`).
- Never use `block_on` inside async contexts.
- Avoid `.unwrap()`/`.expect()` in production code. Use `?`, `ok_or_else`, or explicit fallback behavior. Existing tests may use unwraps when failure would make the test invalid.
- Use `thiserror` for library/domain errors and propagate with the shared `miku_core::Result`.
- Prefer guard clauses, `let-else`, `matches!`, pattern guards, and small helper functions over deeply nested branching.
- Keep public API surfaces small. Add `#[must_use]` when ignoring a return value would likely be a bug.

## Workspace Dependency Management

External dependencies are centralized in the root `Cargo.toml` under `[workspace.dependencies]`. Crates should reference external dependencies with `{ workspace = true }`.

For internal crates, follow the repository's existing pattern of path dependencies such as:

```toml
miku-core = { path = "../miku-core" }
miku-api = { path = "../miku-api" }
```

When adding a new external dependency, add it to the workspace root first, then reference it from member crates. Keep dependency features as narrow as practical, especially for wasm-sensitive crates.

## API and Runtime Boundaries

- `miku-api` defines the contract. Do not put HTTP, kube-rs, rusqlite, egui, or CLI-specific types in public service traits or DTOs unless the contract intentionally depends on them.
- `miku-server` should translate service errors into HTTP responses at the boundary. Keep JSON response shapes explicit and stable.
- `miku-http-client` should mirror `miku-api` contracts for web mode instead of inventing a second UI-facing API.
- `miku-kube` owns kube-rs details, dynamic object handling, resource path mapping, watches, and logs.
- Keep server mode functional without a live Kubernetes client when possible; current startup falls back to offline services.

## Local Data and Persistence Safety

Miku stores user data under `~/.miku` by default:

- `miku.db` for SQLite data.
- `config.toml` for small human-readable settings.
- `logs/` for local logs.
- `cache/` for disposable cache data.

Do not copy Kubernetes credentials into SQLite. Reference existing kubeconfig contexts unless a future secret-store design explicitly changes that policy.

For SQLite changes:

- Keep migrations idempotent and local to `miku-store`.
- Reuse the existing `SqliteStore`/connection ownership model for local persistence.
- Add regression tests for schema, migration, and preference behavior.
- Do not introduce background writers that open independent unmanaged connections to the same database unless there is a clear concurrency design.

## Kubernetes Integration Guidelines

- Treat kubeconfig and cluster access as external, fallible dependencies.
- Surface Kubernetes errors through `MikuError::Kubernetes` with useful context.
- Keep resource identity typed through `ClusterId`, `ResourceRef`, `ResourceScope`, and API DTOs instead of passing ad hoc strings through layers.
- Preserve namespace handling semantics: explicit query namespace overrides resource scope; cluster-scoped resources use `Api::all_with`.
- Avoid blocking calls in Kubernetes paths. Use kube-rs async APIs and streams.
- Add tests for resource path mapping, kind/plural mapping, log parameter mapping, and offline behavior when changing kube integration.

## UI Guidelines (egui)

- `miku-ui` is the shared egui shell for native and web runtimes. Keep native-only behavior behind non-wasm cfg gates.
- Use existing libraries for the UI foundation: `eframe`, `egui_dock`, `egui-theme-switch`, and `egui-phosphor`.
- Keep the docked layout predictable: left cluster navigation, central workspace, right inspector, bottom status bar.
- Use icon buttons with hover text for window/tool actions when an icon exists.
- Do not block egui render/update callbacks on network, disk-heavy, Kubernetes, or server work. Use background tasks, request state, and polling/notifications for long work.
- Keep text and controls sized to fit both desktop and web runtimes. Avoid hard-coded heights except for stable chrome such as the status bar.
- Do not move transport, kube-rs, or store-specific logic into `miku-ui`; use service traits or runtime adapters.

## Web and Wasm Guidelines

- Keep `miku-web` as a thin wasm entrypoint.
- Avoid non-wasm-safe APIs in crates consumed by `miku-web`.
- For cross-runtime service traits, preserve the existing send-bound split:
  - native: `Send + Sync`
  - wasm: no `Send` requirement
- Build wasm-sensitive changes with `rtk cargo build -p miku-web --target wasm32-unknown-unknown`.

## Testing Guidelines

Place unit tests next to implementation (`mod tests`) and integration tests under crate-level `tests/` directories when behavior crosses module boundaries.

Name tests by behavior, for example:

- `no_subcommand_defaults_to_gui`
- `resource_ref_builds_stable_api_path`
- `preferences_round_trip_as_json`

Add regression tests for bug fixes. For changes touching API contracts, persistence, Kubernetes routing, server responses, CLI parsing, or UI state, include focused tests for core paths and important edge cases.

Before completion, run the narrowest meaningful command first, then broaden when risk justifies it. Typical final checks are:

```sh
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
```

## Commit and Pull Request Guidelines

Commit messages follow Conventional Commits. Keep each commit to one logical change.

Format:

```text
<type>(<scope>): <subject>

<body>

<footer>
```

- Subject line: required, imperative mood, lowercase, no trailing period, max 72 chars.
- Body: optional, explains what and why, not how.
- Footer: optional, use for `BREAKING CHANGE:`, `Closes #123`, etc.

Common types:

- `feat`: new feature.
- `fix`: bug fix.
- `docs`: documentation changes.
- `style`: code style only.
- `refactor`: code restructuring without behavior change.
- `perf`: performance improvement.
- `test`: test additions or corrections.
- `chore`: maintenance.
- `ci`: CI/CD configuration.
- `build`: build system or dependency changes.
- `revert`: revert a previous commit.

PRs should include:

- Purpose and impacted crates.
- Test evidence with commands run and results.
- Config, local data, or docs updates when behavior changes.
- Sample CLI output or screenshots/video when user-facing behavior changes.
- Kubernetes/offline-mode notes when cluster access behavior changes.

## Security and Configuration

- Never commit API keys, kubeconfig contents, bearer tokens, certificates, or cluster secrets.
- Do not log sensitive Kubernetes object data by default. Be especially careful with `Secret` resources and raw dynamic object dumps.
- Prefer environment variables and existing kubeconfig resolution for local credentials.
- Redact credentials when sharing configs, logs, or reproduction steps.
