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
    content_html = md_to_html(body)
    content_html = slugify_headers(content_html)
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

    # Static assets
    (DIST / "css").mkdir(parents=True)
    shutil.copy(ROOT / "src/main.css", DIST / "css/main.css")
    shutil.copytree(ROOT / "css", DIST / "css/webtui")
    (DIST / "js").mkdir(parents=True)
    shutil.copy(ROOT / "src/main.js", DIST / "js/main.js")

    count = 0
    for md_file in sorted(CONTENT.rglob("*.md*")):
        rel = md_file.relative_to(CONTENT)
        if rel.name == "index.mdx":
            continue  # landing page handled separately
        meta, body = parse_frontmatter(md_file.read_text())
        url_path = "/" + str(rel.with_suffix("")).replace("\\", "/") + "/"
        out = DIST / str(rel).replace(".mdx", "").replace(".md", "") / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(render_page(meta, body, url_path))
        count += 1

    # Landing page (hand-built, no frontmatter)
    landing = (ROOT / "templates/landing.html").read_text()
    landing = landing.replace("{{ sidebar }}", build_sidebar("/"))
    landing = landing.replace("{{ toc }}", "")
    (DIST / "index.html").write_text(landing)

    print(f"built {count} docs pages + landing → {DIST}")


if __name__ == "__main__":
    build()
