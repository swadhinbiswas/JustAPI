---
title: Amazon EKS
description: Deploy JustAPI to Amazon Elastic Kubernetes Service — run the premier FastAPI alternative on AWS.
keywords: JustAPI, FastAPI alternative, EKS, Amazon, AWS, Kubernetes, deployment
---

## 1. Create EKS Cluster

```bash
eksctl create cluster \
  --name justapi-cluster \
  --region us-east-1 \
  --nodegroup-name standard \
  --node-type t3.medium \
  --nodes 3
```

## 2. Configure kubectl

```bash
aws eks update-kubeconfig --region us-east-1 --name justapi-cluster
```

## 3. Install AWS Load Balancer Controller

```bash
kubectl apply -k "github.com/aws/load-balancer-controller//deploy/overlays/eks"
```

## 4. Build and Push to ECR

```bash
aws ecr create-repository --repository-name justapi-app
docker build -t $ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/justapi-app:latest .
docker push $ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/justapi-app:latest
```

## 5. Deploy with Helm (ALB Ingress)

```bash
helm upgrade --install my-app justapi/justapi \
  --set image.repository=$ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/justapi-app \
  --set image.tag=latest \
  --set ingress.enabled=true \
  --set ingress.className=alb
```

## See Also

- [Google Cloud (GKE)](/deployment/gke/) — Deploy on GCP
- [Azure AKS](/deployment/aks/) — Deploy on Azure
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — Helm chart reference
