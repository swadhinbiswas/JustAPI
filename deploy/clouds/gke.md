# Deploy JustAPI to Google Kubernetes Engine (GKE)

## Prerequisites

- `gcloud` CLI installed and authenticated
- `kubectl` installed
- `helm` installed (v3+)
- A GCP project with billing enabled

## 1. Create a GKE cluster

```bash
PROJECT_ID="your-project-id"
gcloud config set project $PROJECT_ID

gcloud container clusters create justapi-cluster \
  --region us-central1 \
  --num-nodes 3 \
  --machine-type e2-standard-2 \
  --enable-autoscaling --min-nodes 3 --max-nodes 10
```

## 2. Get cluster credentials

```bash
gcloud container clusters get-credentials justapi-cluster --region us-central1
```

## 3. Install NGINX Ingress Controller

```bash
helm upgrade --install ingress-nginx ingress-nginx \
  --repo https://kubernetes.github.io/ingress-nginx \
  --namespace ingress-nginx --create-namespace
```

## 4. Install cert-manager for TLS

```bash
helm repo add jetstack https://charts.jetstack.io
helm repo update
helm upgrade --install cert-manager jetstack/cert-manager \
  --namespace cert-manager --create-namespace \
  --set installCRDs=true
```

## 5. Build and push Docker image

```bash
# Build for linux/amd64
docker build -t gcr.io/$PROJECT_ID/justapi:latest .
docker push gcr.io/$PROJECT_ID/justapi:latest
```

## 6. Deploy with Helm

```bash
helm upgrade --install justapi ./helm/justapi \
  --set image.repository=gcr.io/$PROJECT_ID/justapi \
  --set image.tag=latest \
  --set ingress.hosts[0].host=api.yourdomain.com \
  --set secrets.db.url="postgres://..." \
  --set secrets.redis.url="redis://..."
```

## 7. Expose via Cloud Load Balancer

The NGINX ingress will provision a GCP HTTP(S) Load Balancer automatically.

```bash
kubectl get ingress
# Wait for the ADDRESS field to populate
```

## 8. Configure DNS

Point your domain's A record to the load balancer IP (or use `external-dns` for automatic DNS).

## Verification

```bash
curl https://api.yourdomain.com/health
# {"status":"healthy","components":[]}
curl https://api.yourdomain.com/ready
# {"ready":true}
curl https://api.yourdomain.com/live
# {"alive":true}
```
