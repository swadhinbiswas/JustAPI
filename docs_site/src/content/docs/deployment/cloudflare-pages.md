---
title: Cloudflare Pages Deployment Guide
description: Deploy your JustAPI Astro documentation site to Cloudflare Pages with zero-config CI/CD.
---

This documentation site is built with **Astro** and **Starlight**, making it 100% static-site generator (SSG) compatible and ready for instant deployment on **Cloudflare Pages**.

## Method 1: Git Integration (Recommended)

1. Push your repository to **GitHub** or **GitLab**.
2. Log into the **Cloudflare Dashboard** and navigate to **Workers & Pages**.
3. Click **Create Application** → **Pages** → **Connect to Git**.
4. Select your `JustAPI` repository and choose the branch (e.g. `main` or `initial-setup`).
5. Configure the Build Settings:
   * **Framework Preset:** `Astro`
   * **Root Directory:** `docs_site`
   * **Build Command:** `npm run build`
   * **Build Output Directory:** `dist`
6. Click **Save and Deploy**. Cloudflare Pages will automatically build and publish your site with free global CDN edge distribution and SSL!

## Method 2: Direct CLI Deployment (`wrangler`)

Deploy directly from your local terminal using Cloudflare's `wrangler` CLI:

```bash
# Build the static site output
cd docs_site
npm run build

# Deploy to Cloudflare Pages
npx wrangler pages deploy dist --project-name=justapi-docs
```

## Cloudflare Pages Configuration (`wrangler.jsonc`)

```json
{
  "name": "justapi-docs",
  "pages_build_output_dir": "./dist"
}
```
