# miku-store

`miku-store` owns Miku's local filesystem layout and SQLite-backed persistence. Keep it focused on durable local data under `~/.miku`: database initialization, migrations, schema compatibility, and implementations of persistence traits from `miku-api`.

## Module Boundaries

- `src/paths.rs` resolves and exposes local store paths.
- `src/store.rs` owns `SqliteStore` construction and the shared SeaORM connection.
- `src/clusters.rs` and `src/preferences.rs` define SeaORM entities.
- `src/cluster_registry.rs` and `src/preference_store.rs` implement `miku-api` service traits.
- `src/migrations.rs` defines forward schema migrations.
- `src/schema.rs` contains compatibility repairs for existing local databases.
- `src/util.rs` contains small storage helpers shared across modules.

## Rules

- Do not add UI, HTTP, Kubernetes, or CLI concerns to this crate.
- Expose behavior through `miku-api` traits. Keep SeaORM details private unless an existing public path requires compatibility.
- Prefer migrations for schema changes. Use legacy schema repair only to keep existing user databases opening safely.
- Do not copy kubeconfig credentials, bearer tokens, certificates, Kubernetes Secrets, or raw sensitive object data into SQLite.
- Add focused regression tests for new persistence behavior, schema changes, migrations, and legacy database compatibility.
