---
title: Kubernetes & Helm
description: Deploy JustAPI on Kubernetes using the official Helm chart — the FastAPI alternative built for production.
keywords: JustAPI, FastAPI alternative, Kubernetes, Helm, deployment, production
---

## Prerequisites

- Kubernetes cluster (v1.24+)
- Helm v3.8+
- kubectl configured

## Quick Install

```bash
# Add the JustAPI Helm repository
helm repo add justapi https://justapi.dev/charts
helm repo update

# Install
helm upgrade --install my-app justapi/justapi \
  --set image.repository=myregistry/my-app \
  --set image.tag=latest
```

## Helm Chart Structure

```
helm/justapi/
├── Chart.yaml
├── values.yaml
├── templates/
│   ├── deployment.yaml
│   ├── service.yaml
│   ├── ingress.yaml
│   ├── hpa.yaml
│   ├── configmap.yaml
│   ├── secret.yaml
│   └── serviceaccount.yaml
└── _helpers.tpl
```

## Configuration Reference

| Parameter | Default | Description |
|---|---|---|
| `replicaCount` | `2` | Number of replicas |
| `image.repository` | — | Container image |
| `image.tag` | `latest` | Image tag |
| `image.pullPolicy` | `Always` | Pull policy |
| `service.port` | `8080` | Service port |
| `ingress.enabled` | `false` | Enable ingress |
| `ingress.host` | — | Ingress hostname |
| `ingress.tls` | `false` | Enable TLS |
| `resources.limits.cpu` | `"1000m"` | CPU limit |
| `resources.limits.memory` | `"512Mi"` | Memory limit |
| `hpa.enabled` | `true` | Enable HPA |
| `hpa.minReplicas` | `2` | Min pods |
| `hpa.maxReplicas` | `10` | Max pods |
| `hpa.targetCPUUtilization` | `70` | CPU target % |
| `config.rustLog` | `"info"` | RUST_LOG value |
| `config.otelServiceName` | `"justapi"` | OTel service name |
| `secret.databaseUrl` | — | DB connection string |
| `secret.redisUrl` | — | Redis connection string |

## Deployment with Custom Configuration

```bash
helm upgrade --install my-app justapi/justapi \
  --set replicaCount=4 \
  --set resources.limits.cpu="2000m" \
  --set resources.limits.memory="1Gi" \
  --set hpa.targetCPUUtilization=50 \
  --set config.rustLog="debug" \
  --set ingress.enabled=true \
  --set ingress.host="api.example.com"
```

## Ingress with TLS

```yaml
ingress:
  enabled: true
  host: api.example.com
  tls: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
```

## Health Check Probes

The chart configures Kubernetes probes automatically:

- **Liveness**: `GET /health` — Is the process alive?
- **Readiness**: `GET /ready` — Is the app ready to serve traffic?
- **Startup**: `GET /health` — Has the app finished starting?

## See Also

- [Docker](/deployment/docker/) — Containerize your app
- [Production Checklist](/deployment/production-checklist/) — Production readiness
- [Helm Chart Source](https://github.com/swadhinbiswas/JustAPI/tree/main/helm/justapi)
