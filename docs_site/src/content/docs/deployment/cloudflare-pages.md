---
title: Cloudflare Pages Deployment
description: Deploy your JustAPI documentation site to Cloudflare Pages — fast static hosting for the FastAPI alternative.
keywords: JustAPI, FastAPI alternative, Cloudflare Pages, deployment, documentation, static site
---

This documentation site is built with Astro and Starlight, making it 100% static-site generator (SSG) compatible.

## Method 1: Git Integration (Recommended)

1. Push your repository to GitHub or GitLab
2. Log into Cloudflare Dashboard → Workers & Pages → Create Application → Pages → Connect to Git
3. Select your repository and branch
4. Configure build settings:
   - **Framework Preset:** Astro
   - **Root Directory:** `docs_site`
   - **Build Command:** `npm run build`
   - **Build Output Directory:** `dist`
5. Click **Save and Deploy**

Cloudflare Pages automatically builds and publishes with free global CDN, SSL, and edge distribution.

## Method 2: CLI Deployment (wrangler)

```bash
cd docs_site
npm run build
npx wrangler pages deploy dist --project-name=justapi-docs
```

## Cloudflare Pages Configuration

```json
{
  "name": "justapi-docs",
  "pages_build_output_dir": "./dist"
}
```

## Custom Domain

1. Go to Cloudflare Pages → your project → Custom Domains
2. Add your domain (e.g., `docs.justapi.dev`)
3. Update DNS records

## See Also

- [Docker](/deployment/docker/) — Container deployment
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — K8s deployment
