---
title: Documentation Guide
description: How to contribute to the JustAPI documentation site — a high-performance FastAPI alternative built in Rust.
keywords: documentation guide, contribution, FastAPI alternative, Rust web framework, docs
---

## Tech Stack

The documentation site is built with:

- **Astro** 7.x — Static site generator
- **Starlight** 0.41.x — Documentation theme
- **Pagefind** — Full-text search
- **Markdown/MDX** — Content format

## Local Development

```bash
cd docs_site
npm install
npm run dev
```

The development server starts at `http://localhost:4321`.

## Adding a New Page

1. Create a `.md` or `.mdx` file in the appropriate directory under `src/content/docs/`
2. Add frontmatter:

```yaml
---
title: My New Page
description: Brief description for SEO.
---
```

3. If the page should appear in the sidebar, add it to `astro.config.mjs` in the `sidebar` array

## Page Frontmatter

| Field | Required | Description |
|---|---|---|
| `title` | Yes | Page title (also used in sidebar) |
| `description` | Yes | Meta description for SEO |
| `sidebar` | No | Custom sidebar label (defaults to title) |

## Directory Structure

```
src/content/docs/
├── index.mdx              # Home page (splash layout)
├── getting-started/        # Getting started guides
├── tutorials/              # Step-by-step tutorials
├── api-reference/          # API documentation
├── advanced/               # Advanced guides
├── deployment/             # Deployment guides
├── security/               # Security documentation
├── observability/          # Observability docs
├── reference/              # Reference docs
├── contributing/           # Contributor guides
└── examples/               # Example gallery
```

## Conventions

- Use `#` for page title (H1), `##` for sections (H2), `###` for subsections (H3)
- Code blocks should specify language for syntax highlighting: ` ```python `
- Use relative links to other docs pages: `[Text](/getting-started/overview/)`
- All code examples should be tested before committing

## Building

```bash
npm run build   # Output in dist/
npm run preview # Preview the built site
```

## See Also

- [Development Setup](/contributing/development-setup/) — Get started
- [Coding Standards](/contributing/coding-standards/) — Code conventions
- [Testing Guide](/contributing/testing-guide/) — Writing tests
