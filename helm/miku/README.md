# Miku Helm Chart

This chart deploys Miku server mode with the embedded wasm web UI from GHCR.

## Install

```sh
helm upgrade --install miku ./helm/miku --namespace miku --create-namespace
```

Then open a local port-forward:

```sh
kubectl --namespace miku port-forward svc/miku 8080:80
```

Open http://127.0.0.1:8080.

## Image

The default image is `ghcr.io/zhubby/miku:v0.2.0`. Override it with:

```sh
helm upgrade --install miku ./helm/miku \
  --namespace miku \
  --create-namespace \
  --set image.tag=v0.2.0
```

## RBAC

Miku is a Kubernetes management UI. By default, the chart creates a service account with
cluster-wide permissions so resource listing, logs, exec, apply, patch, delete, cordon, drain, and
eviction workflows can operate.

To use an existing service account:

```yaml
serviceAccount:
  create: false
  name: miku
rbac:
  create: false
```

## Persistence

By default, `/data` uses an ephemeral `emptyDir`. Enable a PVC for persistent local settings and
SQLite data:

```yaml
persistence:
  enabled: true
  size: 1Gi
```
