#!/usr/bin/env python3
"""JustAPI docs build — WebTUI terminal static site generator.

Reads markdown from ../docs_site/src/content/docs, renders each page with
the WebTUI terminal template (navbar + dual sidebars + content), and writes
static HTML to dist/.
"""
import re
import shutil
import html as html_mod
from pathlib import Path

import markdown

ROOT = Path(__file__).parent
CONTENT = Path(__file__).parent.parent / "docs_site/src/content/docs"
DIST = ROOT / "dist"

# ─── Sidebar structure: (group, [(label, path), ...]) ───
SIDEBAR = [
    ("Getting Started", [
        ("Overview", "/getting-started/overview/"),
        ("Installation", "/getting-started/installation/"),
        ("First Steps", "/getting-started/first-steps/"),
        ("CLI Scaffolder", "/getting-started/cli-scaffolder/"),
        ("Migrate from FastAPI", "/getting-started/migrating-from-fastapi/"),
    ]),
    ("Tutorials", [
        ("Hello World", "/tutorials/hello-world/"),
        ("Path Parameters", "/tutorials/path-params/"),
        ("Query Parameters", "/tutorials/query-params/"),
        ("Request Body", "/tutorials/request-body/"),
        ("Error Handling", "/tutorials/error-handling/"),
        ("Dependencies", "/tutorials/dependency-injection/"),
        ("Middleware", "/tutorials/middleware/"),
        ("CORS", "/tutorials/cors/"),
        ("Databases", "/tutorials/database-integration/"),
        ("Background Tasks", "/tutorials/background-tasks/"),
        ("Testing", "/tutorials/testing/"),
        ("Static Files", "/tutorials/static-files/"),
    ]),
    ("Advanced", [
        ("Zero-GIL", "/advanced/zero-gil-architecture/"),
        ("Rust Core", "/advanced/rust-core-deep-dive/"),
        ("Native Fast Path", "/advanced/native-fast-path/"),
        ("Streaming Output", "/advanced/streaming-output/"),
        ("WebSockets", "/advanced/websockets-advanced/"),
        ("Templates", "/advanced/templates/"),
        ("Performance Tuning", "/advanced/performance-tuning/"),
        ("Resilience", "/advanced/resilience-patterns/"),
    ]),
    ("API Reference", [
        ("JustAPIApp", "/api-reference/justapiapp/"),
        ("Routing", "/api-reference/routing/"),
        ("Request", "/api-reference/request/"),
        ("Responses", "/api-reference/responses/"),
        ("Dependency Injection", "/api-reference/dependency-injection/"),
        ("Exceptions", "/api-reference/exceptions/"),
        ("Schema Validation", "/api-reference/schema-validation/"),
        ("WebSockets", "/api-reference/websockets/"),
        ("Background Tasks", "/api-reference/background-tasks/"),
        ("Scheduler", "/api-reference/scheduler/"),
        ("Database", "/api-reference/database/"),
        ("Testing Client", "/api-reference/testing-client/"),
        ("UploadFile", "/api-reference/uploadfile/"),
    ]),
    ("Deployment", [
        ("Docker", "/deployment/docker/"),
        ("Kubernetes", "/deployment/kubernetes-helm/"),
        ("Cloudflare Pages", "/deployment/cloudflare-pages/"),
        ("Production Checklist", "/deployment/production-checklist/"),
    ]),
    ("Security", [
        ("Security Policy", "/security/policy/"),
        ("OWASP", "/security/owasp-compliance/"),
        ("Secure Config", "/security/secure-configuration/"),
    ]),
    ("Observability", [
        ("Metrics", "/observability/metrics-monitoring/"),
        ("OpenTelemetry", "/observability/opentelemetry/"),
        ("Structured Logging", "/observability/structured-logging/"),
        ("Health Checks", "/observability/health-checks/"),
    ]),
    ("Reference", [
        ("CLI", "/reference/cli/"),
        ("Configuration", "/reference/configuration/"),
        ("API Stability", "/reference/api-stability/"),
        ("Release Notes", "/reference/release-notes/"),
        ("Glossary", "/reference/glossary/"),
    ]),
]

TEMPLATE = (ROOT / "templates/base.html").read_text()


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
                        continue  # skip list/dict meta
                    meta[k.strip()] = v
            text = text[end + 3 :]
    return meta, text


def build_sidebar(active_path: str) -> str:
    parts = []
    for group, items in SIDEBAR:
        parts.append(f'<div class="ja-nav-group">{html_mod.escape(group)}</div>')
        parts.append('<ul marker-="tree">')
        for label, path in items:
            cls = ' class="active"' if path == active_path else ""
            parts.append(f'  <li><a href="{path}"{cls}>{html_mod.escape(label)}</a></li>')
        parts.append('</ul>')
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
    html = markdown.markdown(
        md_text,
        extensions=["fenced_code", "tables", "codehilite", "toc", "attr_list"],
    )
    # Enrich standard elements with WebTUI attributes
    # <pre> left unstyled — site CSS handles code block styling
    html = html.replace("<table>", '<table class="shadow" box-="square">')
    html = html.replace("<blockquote>", '<blockquote box-="square">')
    return html


def get_prev_next(active_path: str):
    flat = []
    for group, items in SIDEBAR:
        for label, path in items:
            flat.append((label, path))
    for i, (label, path) in enumerate(flat):
        if path == active_path:
            prev_item = flat[i-1] if i > 0 else None
            next_item = flat[i+1] if i < len(flat) - 1 else None
            return prev_item, next_item
    return None, None


def render_page(meta: dict, body: str, path: str) -> str:
    template = (ROOT / "templates/base.html").read_text()
    content_html = md_to_html(body)
    content_html = slugify_headers(content_html)
    
    prev_item, next_item = get_prev_next(path)
    if prev_item or next_item:
        nav_html = '\n<div class="ja-doc-nav">\n'
        if prev_item:
            nav_html += f'  <a href="{prev_item[1]}" class="ja-doc-nav-prev"><span class="nf">&#xf060;</span> Prev: {html_mod.escape(prev_item[0])}</a>\n'
        else:
            nav_html += '  <span></span>\n'
        if next_item:
            nav_html += f'  <a href="{next_item[1]}" class="ja-doc-nav-next">Next: {html_mod.escape(next_item[0])} <span class="nf">&#xf061;</span></a>\n'
        nav_html += '</div>\n'
        content_html += nav_html

    title = meta.get("title", "JustAPI")
    description = meta.get("description", "JustAPI — Python web framework with a Rust core.")
    page = template
    page = page.replace("{{ title }}", html_mod.escape(title))
    page = page.replace("{{ description }}", html_mod.escape(description))
    page = page.replace("{{ content }}", content_html)
    page = page.replace("{{ sidebar }}", build_sidebar(path))
    page = page.replace("{{ toc }}", build_toc(content_html))
    page = page.replace("{{ url }}", path)
    return page


def build() -> None:
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir(parents=True)

    # Static assets
    (DIST / "css").mkdir(parents=True)
    shutil.copy(ROOT / "src/main.css", DIST / "css/main.css")
    shutil.copytree(ROOT / "css", DIST / "css/webtui")
    (DIST / "js").mkdir(parents=True)
    shutil.copy(ROOT / "src/main.js", DIST / "js/main.js")

    count = 0
    urls = ["/"]
    all_llm_content = ["# JustAPI Full Documentation\n\n"]
    for md_file in sorted(CONTENT.rglob("*.md*")):
        rel = md_file.relative_to(CONTENT)
        if rel.name == "index.mdx":
            continue  # landing page handled separately
        meta, body = parse_frontmatter(md_file.read_text())
        url_path = "/" + str(rel.with_suffix("")).replace("\\", "/") + "/"
        out = DIST / str(rel).replace(".mdx", "").replace(".md", "") / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(render_page(meta, body, url_path))
        urls.append(url_path)
        all_llm_content.append(f"## {meta.get('title', url_path)}\n\n{body}\n\n")
        count += 1

    # Landing page (hand-built, no frontmatter)
    landing = (ROOT / "templates/landing.html").read_text()
    landing = landing.replace("{{ sidebar }}", build_sidebar("/"))
    landing = landing.replace("{{ toc }}", "")
    (DIST / "index.html").write_text(landing)

    # Generate Section Index Entrypoint Aliases (so /getting-started/, /tutorials/, etc. work directly)
    SECTION_ALIASES = [
        ("getting-started", "getting-started/overview"),
        ("tutorials", "tutorials/hello-world"),
        ("api-reference", "api-reference/justapiapp"),
        ("deployment", "deployment/docker"),
        ("advanced", "advanced/zero-gil-architecture"),
        ("security", "security/policy"),
        ("observability", "observability/metrics-monitoring"),
        ("reference", "reference/cli"),
    ]
    for sec, target in SECTION_ALIASES:
        sec_dir = DIST / sec
        target_file = DIST / target / "index.html"
        if target_file.exists():
            sec_dir.mkdir(parents=True, exist_ok=True)
            shutil.copy(target_file, sec_dir / "index.html")

    # Generate sitemap.xml
    sitemap = ['<?xml version="1.0" encoding="UTF-8"?>']
    sitemap.append('<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">')
    for u in urls:
        priority = "1.0" if u == "/" else "0.8"
        sitemap.append(f'  <url>\n    <loc>https://justapi.pages.dev{u}</loc>\n    <changefreq>weekly</changefreq>\n    <priority>{priority}</priority>\n  </url>')
    sitemap.append('</urlset>')
    (DIST / "sitemap.xml").write_text("\n".join(sitemap))

    # Generate robots.txt
    robots = "User-agent: *\nAllow: /\n\nSitemap: https://justapi.pages.dev/sitemap.xml\n"
    (DIST / "robots.txt").write_text(robots)

    # Generate all-llm.txt
    (DIST / "all-llm.txt").write_text("".join(all_llm_content))

    print(f"built {count} docs pages + landing + section entrypoints + seo artifacts → {DIST}")


if __name__ == "__main__":
    build()
