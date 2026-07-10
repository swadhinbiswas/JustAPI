# Deploy JustAPI to Azure Kubernetes Service (AKS)

## Prerequisites

- `az` CLI installed and authenticated
- `kubectl` installed
- `helm` installed (v3+)

## 1. Create an AKS cluster

```bash
RESOURCE_GROUP="justapi-rg"
CLUSTER_NAME="justapi-aks"

az group create --name $RESOURCE_GROUP --location eastus

az aks create \
  --resource-group $RESOURCE_GROUP \
  --name $CLUSTER_NAME \
  --node-count 3 \
  --enable-cluster-autoscaler \
  --min-count 2 \
  --max-count 10 \
  --node-vm-size Standard_D2s_v3 \
  --generate-ssh-keys
```

## 2. Get credentials

```bash
az aks get-credentials --resource-group $RESOURCE_GROUP --name $CLUSTER_NAME
```

## 3. Install NGINX Ingress Controller

```bash
helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
helm repo update

helm upgrade --install ingress-nginx ingress-nginx/ingress-nginx \
  --namespace ingress-nginx --create-namespace
```

## 4. Install cert-manager

```bash
helm repo add jetstack https://charts.jetstack.io
helm repo update
helm upgrade --install cert-manager jetstack/cert-manager \
  --namespace cert-manager --create-namespace \
  --set installCRDs=true
```

## 5. Build and push to Azure Container Registry

```bash
# Create ACR if needed
az acr create --resource-group $RESOURCE_GROUP --name justapiacr --sku Basic
az acr login --name justapiacr

docker build -t justapiacr.azurecr.io/justapi:latest .
docker push justapiacr.azurecr.io/justapi:latest
```

## 6. Attach ACR to AKS

```bash
az aks update --resource-group $RESOURCE_GROUP --name $CLUSTER_NAME --attach-acr justapiacr
```

## 7. Deploy with Helm

```bash
helm upgrade --install justapi ./helm/justapi \
  --set image.repository=justapiacr.azurecr.io/justapi \
  --set image.tag=latest \
  --set secrets.db.url="postgres://..." \
  --set secrets.redis.url="redis://..."
```

## Verification

```bash
kubectl get svc ingress-nginx-controller -n ingress-nginx
# Get the external IP
curl http://<external-ip>/health
```
