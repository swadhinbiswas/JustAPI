#!/usr/bin/env python3
"""JustAPI docs build — WebTUI terminal static site generator.

Reads markdown from ../docs_site/src/content/docs, renders each page with
the WebTUI terminal template (navbar + dual sidebars + content), and writes
static HTML to dist/.

The sidebar is auto-discovered from the content directory, so every page
is always listed — nothing hardcoded.
"""
import re
import shutil
import html as html_mod
from pathlib import Path

import markdown

ROOT = Path(__file__).parent
CONTENT = Path(__file__).parent.parent / "docs_site/src/content/docs"
DIST = ROOT / "dist"

# Section order + display names for top-level groups.
SECTION_ORDER = [
    "getting-started", "tutorials", "advanced", "how-to",
    "api-reference", "deployment", "security", "observability",
    "inference", "resources", "reference", "contributing", "examples",
]
SECTION_LABELS = {
    "getting-started": "Getting Started",
    "tutorials": "Tutorials",
    "advanced": "Advanced",
    "how-to": "How-To",
    "api-reference": "API Reference",
    "deployment": "Deployment",
    "security": "Security",
    "observability": "Observability",
    "inference": "Inference",
    "resources": "Resources",
    "reference": "Reference",
    "contributing": "Contributing",
    "examples": "Examples",
}

SKIP_STEMS = {"index"}

TEMPLATE = (ROOT / "templates/base.html").read_text()


def humanize(name: str) -> str:
    """'path-params-numeric-validations' -> 'Path Params Numeric Validations'."""
    return name.replace("-", " ").replace("_", " ").title().replace(" Api ", " API ")


def frontmatter_title(text: str, fallback: str) -> str:
    """Read the `title:` from YAML frontmatter, else humanized filename."""
    if text.startswith("---"):
        end = text.find("---", 3)
        if end != -1:
            for line in text[3:end].splitlines():
                if line.startswith("title:"):
                    t = line.split(":", 1)[1].strip().strip('"').strip("'")
                    if t:
                        return t
    return fallback


def discover_pages() -> list[tuple[str, list[tuple[str, str]]]]:
    """Scan CONTENT -> [(group_label, [(page_label, url_path)])] in order."""
    groups: dict[str, list[tuple[str, str]]] = {}
    for md in sorted(CONTENT.rglob("*.md*")):
        rel = md.relative_to(CONTENT)
        parts = rel.parts
        if len(parts) < 2:
            continue
        section = parts[0]
        stem = rel.stem
        if len(parts) == 2 and stem in SKIP_STEMS:
            continue  # section index pages (how-to/index.md) ARE built
        label = frontmatter_title(md.read_text(errors="ignore"), humanize(stem))
        url = "/" + str(rel.with_suffix("")).replace(chr(92), "/") + "/"
        groups.setdefault(section, []).append((label, url))

    ordered = []
    for sec in SECTION_ORDER:
        if sec in groups:
            ordered.append((SECTION_LABELS.get(sec, humanize(sec)), groups[sec]))
    for sec in groups:
        if sec not in SECTION_ORDER:
            ordered.append((SECTION_LABELS.get(sec, humanize(sec)), groups[sec]))
    return ordered


def parse_frontmatter(text: str) -> tuple[dict, str]:
    """Extract YAML-ish frontmatter (title/description) and the body."""
    meta: dict = {}
    if text.startswith("---"):
        end = text.find("---", 3)
        if end != -1:
            raw = text[3:end]
            for line in raw.strip().splitlines():
                if ":" in line:
                    k, v = line.split(":", 1)
                    v = v.strip().strip('"').strip("'")
                    if v.startswith("[") or v.startswith("{"):
                        continue
                    meta[k.strip()] = v
            text = text[end + 3 :]
    return meta, text


def build_sidebar(active_path: str) -> str:
    """Render the sidebar: group headings + links from auto-discovery."""
    parts = []
    for group, items in discover_pages():
        parts.append(f'<div class="ja-nav-group">{html_mod.escape(group)}</div>')
        for label, path in items:
            cls = ' class="active"' if path == active_path else ""
            parts.append(f'<a href="{path}"{cls}>{html_mod.escape(label)}</a>')
    return "\n".join(parts)


def build_toc(html: str) -> str:
    """Extract h2/h3 headings from rendered HTML for the right-side TOC."""
    links = []
    for m in re.finditer(r"<h([23])[^>]*>(.*?)</h\1>", html, re.S):
        level, title = m.group(1), re.sub(r"<[^>]+>", "", m.group(2))
        slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
        links.append(f'<a class="h{level}" href="#{slug}">{html_mod.escape(title)}</a>')
    return "\n".join(links)


def slugify_headers(html: str) -> str:
    """Add id attributes to h2/h3 for TOC anchors."""

    def repl(m):
        level, attrs, title = m.group(1), m.group(2), m.group(3)
        slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
        return f'<h{level} id="{slug}"{attrs}>{title}</h{level}>'

    return re.sub(r"<h([23])([^>]*)>(.*?)</h\1>", repl, html, flags=re.S)


def md_to_html(md_text: str) -> str:
    return markdown.markdown(
        md_text,
        extensions=["fenced_code", "tables", "codehilite", "toc", "attr_list"],
    )


def render_page(meta: dict, body: str, path: str) -> str:
    content_html = slugify_headers(md_to_html(body))
    title = meta.get("title", "JustAPI")
    description = meta.get("description", "JustAPI — Python web framework with a Rust core.")
    page = TEMPLATE
    page = page.replace("{{ title }}", html_mod.escape(title))
    page = page.replace("{{ description }}", html_mod.escape(description))
    page = page.replace("{{ content }}", content_html)
    page = page.replace("{{ sidebar }}", build_sidebar(path))
    page = page.replace("{{ toc }}", build_toc(content_html))
    return page


def build() -> None:
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir(parents=True)

    (DIST / "css").mkdir(parents=True)
    shutil.copy(ROOT / "src/main.css", DIST / "css/main.css")
    shutil.copytree(ROOT / "css", DIST / "css/webtui")
    (DIST / "js").mkdir(parents=True)
    shutil.copy(ROOT / "src/main.js", DIST / "js/main.js")

    count = 0
    for md_file in sorted(CONTENT.rglob("*.md*")):
        rel = md_file.relative_to(CONTENT)
        if len(rel.parts) == 1:
            continue  # top-level index.mdx = landing page
        meta, body = parse_frontmatter(md_file.read_text())
        url_path = "/" + str(rel.with_suffix("")).replace(chr(92), "/") + "/"
        out = DIST / str(rel).replace(".mdx", "").replace(".md", "") / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(render_page(meta, body, url_path))
        count += 1

    landing = (ROOT / "templates/landing.html").read_text()
    landing = landing.replace("{{ sidebar }}", build_sidebar("/"))
    landing = landing.replace("{{ toc }}", "")
    (DIST / "index.html").write_text(landing)

    # ─── Sitemap + robots (fixes "could not fetch sitemap" for search engines) ───
    SITE = "https://justapi.pages.dev"
    urls = [f"<url><loc>{SITE}/</loc></url>"]
    for md_file in sorted(CONTENT.rglob("*.md*")):
        rel = md_file.relative_to(CONTENT)
        if len(rel.parts) == 1:
            continue
        path = "/" + str(rel.with_suffix("")).replace(chr(92), "/") + "/"
        urls.append(f"<url><loc>{SITE}{path}</loc></url>")
    sitemap = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + "\n".join(urls)
        + "\n</urlset>\n"
    )
    (DIST / "sitemap.xml").write_text(sitemap)
    (DIST / "robots.txt").write_text(
        "User-agent: *\nAllow: /\n\nSitemap: " + SITE + "/sitemap.xml\n"
    )

    print(f"built {count} docs pages + landing -> {DIST}")

    # sitemap has 1 (landing) + count URLs
    print(f"sitemap.xml: {len(urls)} URLs")


if __name__ == "__main__":
    build()
