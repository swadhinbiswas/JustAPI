---
title: Google Cloud (GKE)
description: Deploy JustAPI to Google Kubernetes Engine.
---

## 1. Create GKE Cluster

```bash
gcloud container clusters create justapi-cluster \
  --zone us-central1-a \
  --num-nodes 3 \
  --machine-type e2-standard-2
```

## 2. Get Credentials

```bash
gcloud container clusters get-credentials justapi-cluster --zone us-central1-a
```

## 3. Install NGINX Ingress Controller

```bash
kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/cloud/deploy.yaml
```

## 4. Install cert-manager

```bash
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/latest/download/cert-manager.yaml
```

## 5. Build and Push to GCR

```bash
docker build -t gcr.io/$PROJECT_ID/justapi-app:latest .
docker push gcr.io/$PROJECT_ID/justapi-app:latest
```

## 6. Deploy with Helm

```bash
helm upgrade --install my-app justapi/justapi \
  --set image.repository=gcr.io/$PROJECT_ID/justapi-app \
  --set image.tag=latest \
  --set ingress.enabled=true \
  --set ingress.host=api.example.com
```

## See Also

- [Amazon EKS](/deployment/eks/) — Deploy on AWS
- [Azure AKS](/deployment/aks/) — Deploy on Azure
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — Helm chart reference
