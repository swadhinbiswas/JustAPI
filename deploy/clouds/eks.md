# Deploy JustAPI to Amazon EKS

## Prerequisites

- `aws` CLI installed and configured
- `eksctl` installed
- `kubectl` installed
- `helm` installed (v3+)

## 1. Create an EKS cluster

```bash
CLUSTER_NAME="justapi-cluster"
REGION="us-west-2"

eksctl create cluster \
  --name $CLUSTER_NAME \
  --region $REGION \
  --nodegroup-name standard-workers \
  --node-type t3.medium \
  --nodes 3 \
  --nodes-min 2 \
  --nodes-max 10 \
  --managed
```

## 2. Configure kubectl

```bash
aws eks update-kubeconfig --region $REGION --name $CLUSTER_NAME
```

## 3. Install AWS Load Balancer Controller

```bash
helm repo add eks https://aws.github.io/eks-charts
helm repo update

helm upgrade --install aws-load-balancer-controller eks/aws-load-balancer-controller \
  --namespace kube-system \
  --set clusterName=$CLUSTER_NAME
```

## 4. Build and push to ECR

```bash
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
aws ecr create-repository --repository-name justapi --region $REGION || true

docker build -t $ACCOUNT_ID.dkr.ecr.$REGION.amazonaws.com/justapi:latest .
aws ecr get-login-password --region $REGION | \
  docker login --username AWS --password-stdin $ACCOUNT_ID.dkr.ecr.$REGION.amazonaws.com
docker push $ACCOUNT_ID.dkr.ecr.$REGION.amazonaws.com/justapi:latest
```

## 5. Deploy with Helm

```bash
helm upgrade --install justapi ./helm/justapi \
  --set image.repository=$ACCOUNT_ID.dkr.ecr.$REGION.amazonaws.com/justapi \
  --set image.tag=latest \
  --set ingress.className=alb \
  --set ingress.annotations."alb\.ingress\.kubernetes\.io/scheme"=internet-facing \
  --set ingress.annotations."alb\.ingress\.kubernetes\.io/target-type"=ip \
  --set secrets.db.url="postgres://..." \
  --set secrets.redis.url="redis://..."
```

## Verification

```bash
kubectl get ingress
# The ALB DNS name will appear in the ADDRESS field
curl https://<alb-dns>/health
```
