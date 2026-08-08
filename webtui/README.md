# JustAPI Docs — WebTUI Terminal Site

A from-scratch documentation site built on **WebTUI** (https://webtui.ironclad.sh)
— the pure-CSS library that brings Terminal UIs to the browser.

## Structure

```
webtui/
├── build.py            # static site generator (markdown → HTML)
│                       #   · sidebar auto-discovered from content dir (never stale)
│                       #   · 160 pages = docs_site parity
├── css/                # vendored WebTUI (base, components, nf, themes)
├── src/
│   ├── main.css        # terminal design system (navbar, dual sidebar, landing)
│   └── main.js         # theme switcher, tabs, bars, sidebar active states
├── templates/
│   ├── base.html       # page template (navbar + left nav + content + right TOC)
│   └── landing.html    # home page (hero, stats, terminal, bars, cards)
├── content/            # (empty — source markdown lives in ../docs_site/src/content/docs)
└── dist/               # generated static site (deploy this)
```

## Build

```bash
python3 build.py        # reads ../docs_site/src/content/docs → dist/
```

Output: static HTML with zero JS framework — WebTUI is pure CSS + a small
vanilla JS file for theme switching and the terminal interactions.

## Design

- **8-bit terminal palette** — classic ANSI colors (`--bit0`..`--bit15`)
- **Themes**: dark · light · catppuccin (default) · everforest · nord
  (floating switcher, bottom-right)
- **Nerd Font icons** — Symbols Nerd Font via CDN + vendored `nf.css`
- **Font**: JetBrains Mono + Symbols Nerd Font fallback
- **Layout**: terminal title-bar navbar, left docs sidebar, content, right
  per-page TOC — the classic dual-sidebar docs layout, terminal-styled

## Deploy

```bash
npx wrangler pages deploy dist --project-name=justapi
```
