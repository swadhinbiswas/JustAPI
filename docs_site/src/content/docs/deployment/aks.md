---
title: Azure AKS
description: Deploy JustAPI to Azure Kubernetes Service.
---

## 1. Create AKS Cluster

```bash
az group create --name justapi-rg --location eastus
az aks create \
  --resource-group justapi-rg \
  --name justapi-cluster \
  --node-count 3 \
  --enable-managed-identity
```

## 2. Get Credentials

```bash
az aks get-credentials --resource-group justapi-rg --name justapi-cluster
```

## 3. Install NGINX Ingress Controller

```bash
kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/cloud/deploy.yaml
```

## 4. Install cert-manager

```bash
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/latest/download/cert-manager.yaml
```

## 5. Build and Push to ACR

```bash
az acr create --resource-group justapi-rg --name justapiacr --sku Basic
az acr build --registry justapiacr --image justapi-app:latest .
```

## 6. Attach ACR to AKS

```bash
az aks update --resource-group justapi-rg --name justapi-cluster --attach-acr justapiacr
```

## 7. Deploy with Helm

```bash
helm upgrade --install my-app justapi/justapi \
  --set image.repository=justapiacr.azurecr.io/justapi-app \
  --set image.tag=latest \
  --set ingress.enabled=true \
  --set ingress.host=api.example.com
```

## See Also

- [Google Cloud (GKE)](/deployment/gke/) — Deploy on GCP
- [Amazon EKS](/deployment/eks/) — Deploy on AWS
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — Helm chart reference
