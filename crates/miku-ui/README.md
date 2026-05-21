# Resource Panel Module

This module owns the reusable resource-list UI framework inside `miku-ui`.
It is the UI-side reference for Kubernetes resource panels such as Pods,
Deployments, Services, and future resource lists.

## Module Layout

- `mod.rs`: shared resource-panel request/event types, load status, and namespace list helpers.
- `components/`: reusable UI building blocks shared by resource panels.
- `components/toolbar.rs`: namespace selector, name search input, refresh button, item count, and loading label.
- `components/yaml_dialog.rs`: reusable read-only and editable YAML dialogs for resource manifests.
- `pod.rs`: the first concrete resource panel. It loads Pods, parses Pod-specific fields, and renders the Pod table with `egui_extras::TableBuilder`.

Keep reusable widgets under `components/`. Keep resource-specific state,
Kubernetes JSON parsing, table columns, and tests in that resource's file.

## Crate Boundaries

- `miku-ui`: renders egui panels, owns UI state, filters rows, and converts `ResourceSummary.raw` into display rows.
- `miku-api`: defines runtime-neutral service contracts and DTOs such as `ResourceQuery`, `ResourceList`, and `ResourceSummary`.
- `miku-core`: defines stable domain identifiers and resource references such as `ClusterId` and `ResourceRef`.
- `miku-kube`: implements Kubernetes access with kube-rs and returns API DTOs through `miku-api` traits.
- `miku-server`: exposes service traits over HTTP, including `POST /api/resources/list`.
- `miku-http-client`: mirrors `miku-api` contracts for web/BS mode.
- `miku-web`: starts the wasm egui app and provides an HTTP-backed service implementation to `miku-ui`.
- `miku-cli`: launches native GUI or server mode.
- `miku-store`: owns local SQLite persistence and is not part of resource table rendering.

Resource panels should not depend on kube-rs, HTTP types, SQLite, or CLI
details. They should request data through `miku-api` contracts and render only
from DTOs returned by those services.
