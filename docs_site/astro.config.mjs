import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import sitemap from '@astrojs/sitemap';

const site = 'https://justapi.pages.dev';

export default defineConfig({
  site,
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: 'JustAPI',
      expressiveCode: {
        themes: ['dracula', 'github-light'],
        styleOverrides: {
          borderRadius: '12px',
        },
      },
      description: 'Python web framework. Rust engine underneath.',
      logo: {
        src: './src/assets/logo.svg',
        replacesTitle: false,
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/swadhinbiswas/JustAPI' },
        { icon: 'x.com', label: 'X (Twitter)', href: 'https://x.com/justapidev' },
      ],
      customCss: ['./src/styles/custom.css'],
      editLink: {
        baseUrl: 'https://github.com/swadhinbiswas/JustAPI/edit/main/docs_site',
      },
      head: [
        {
          tag: 'script',
          attrs: { is: 'inline' },
          content: `// JustAPI theme switcher (dark/light/dracula/rose-pine/warm)
(function () {
  var THEMES = ['dark', 'light', 'dracula', 'rose-pine', 'warm'];
  function apply(t) {
    document.documentElement.setAttribute('data-theme', t);
    try { localStorage.setItem('ja-theme', t); } catch (e) {}
    document.querySelectorAll('.ja-theme-btn').forEach(function (b) {
      b.setAttribute('data-active', b.dataset.theme === t ? 'true' : 'false');
    });
  }
  document.addEventListener('DOMContentLoaded', function () {
    var saved = null;
    try { saved = localStorage.getItem('ja-theme'); } catch (e) {}
    if (saved && THEMES.indexOf(saved) !== -1) apply(saved);
    var w = document.createElement('div');
    w.className = 'ja-theme-switcher';
    THEMES.forEach(function (t) {
      var b = document.createElement('button');
      b.className = 'ja-theme-btn ja-theme-btn--' + (t === 'rose-pine' ? 'rosepine' : t);
      b.dataset.theme = t;
      b.title = t;
      b.setAttribute('aria-label', 'Theme: ' + t);
      b.addEventListener('click', function () { apply(t); });
      w.appendChild(b);
    });
    document.body.appendChild(w);
    apply(saved || (window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'));
  });
})();`,
        },
        { tag: 'meta', attrs: { charset: 'utf-8' } },
        { tag: 'meta', attrs: { name: 'viewport', content: 'width=device-width, initial-scale=1' } },
        { tag: 'meta', attrs: { name: 'generator', content: 'Astro v7 + Starlight' } },
        { tag: 'meta', attrs: { name: 'robots', content: 'index, follow, max-snippet:-1, max-image-preview:large' } },
        { tag: 'meta', attrs: { name: 'keywords', content: 'JustAPI, FastAPI alternative, Python web framework, Rust, API, async, high performance' } },
        { tag: 'link', attrs: { rel: 'canonical', href: site } },
        { tag: 'meta', attrs: { property: 'og:type', content: 'website' } },
        { tag: 'meta', attrs: { property: 'og:url', content: site } },
        { tag: 'meta', attrs: { property: 'og:site_name', content: 'JustAPI' } },
        { tag: 'meta', attrs: { property: 'og:title', content: 'JustAPI — Built to Handle Pressure' } },
        { tag: 'meta', attrs: { property: 'og:description', content: 'Python web framework powered by Rust. Handles 700k+ requests per second. FastAPI alternative with 20× performance.' } },
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://justapi.pages.dev/og-image.png' } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        { tag: 'meta', attrs: { property: 'og:image:alt', content: 'JustAPI — Python web framework powered by Rust' } },
        { tag: 'meta', attrs: { property: 'og:locale', content: 'en_US' } },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
        { tag: 'meta', attrs: { name: 'twitter:title', content: 'JustAPI — Built to Handle Pressure' } },
        { tag: 'meta', attrs: { name: 'twitter:description', content: 'Python web framework powered by Rust. 700k+ requests per second. FastAPI alternative.' } },
        { tag: 'meta', attrs: { name: 'twitter:image', content: 'https://justapi.pages.dev/og-image.png' } },
        {
          tag: 'script',
          attrs: { type: 'application/ld+json' },
          content: JSON.stringify({
            '@context': 'https://schema.org',
            '@graph': [
              {
                '@type': 'WebSite',
                '@id': `${site}/#website`,
                url: site,
                name: 'JustAPI',
                description: 'Rust-powered Python web framework. A drop-in replacement for FastAPI with 20x performance.',
                inLanguage: 'en-US',
                publisher: { '@id': `${site}/#organization` },
              },
              {
                '@type': 'Organization',
                '@id': `${site}/#organization`,
                name: 'JustAPI',
                url: site,
                logo: `${site}/logo.svg`,
                description: 'JustAPI is a high-performance Python web framework built on a Rust runtime engine.',
              },
              {
                '@type': 'SoftwareApplication',
                '@id': `${site}/#software`,
                name: 'JustAPI',
                applicationCategory: 'WebApplication',
                operatingSystem: 'Linux, macOS, Windows',
                description: 'A high-performance Python web framework built on Rust. Drop-in FastAPI replacement with 700k+ RPS, zero-GIL execution, and native database, GraphQL, gRPC support.',
                url: site,
                downloadUrl: 'https://pypi.org/project/justapi/',
                installUrl: 'https://pypi.org/project/justapi/',
                softwareVersion: '2.0.9',
                offers: {
                  '@type': 'Offer',
                  price: '0',
                  priceCurrency: 'USD',
                },
                author: { '@id': `${site}/#organization` },
              },
              {
                '@type': 'BreadcrumbList',
                '@id': `${site}/#breadcrumb`,
                itemListElement: [
                  { '@type': 'ListItem', position: 1, name: 'Home', item: site },
                  { '@type': 'ListItem', position: 2, name: 'Documentation', item: `${site}/getting-started/overview/` },
                ],
              },
            ],
          }),
        },
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Overview & Philosophy', link: 'getting-started/overview/' },
            { label: 'Installation', link: 'getting-started/installation/' },
            { label: 'First Steps', link: 'getting-started/first-steps/' },
            { label: 'CLI Project Scaffolder', link: 'getting-started/cli-scaffolder/' },
            { label: 'Migrating from FastAPI', link: 'getting-started/migrating-from-fastapi/' },
            { label: 'Migration from Robyn/Granian', link: 'getting-started/migration-guide/' },
          ],
        },
        {
          label: 'Tutorial — User Guide',
          badge: { text: 'Start here', variant: 'success' },
          items: [
            { label: 'Python Types Intro', link: 'tutorials/python-types/' },
            { label: 'Async / Await', link: 'tutorials/async-await/' },
            { label: 'First Steps', link: 'tutorials/hello-world/' },
            { label: 'Path Parameters', link: 'tutorials/path-params/' },
            { label: 'Query Parameters', link: 'tutorials/query-params/' },
            { label: 'Request Body (Pydantic)', link: 'tutorials/request-body/' },
            { label: 'Query Parameters & String Validations', link: 'tutorials/query-params-str-validations/' },
            { label: 'Path Parameters & Numeric Validations', link: 'tutorials/path-params-numeric-validations/' },
            { label: 'Body — Multiple Parameters', link: 'tutorials/body-multiple-params/' },
            { label: 'Body — Fields', link: 'tutorials/body-fields/' },
            { label: 'Body — Nested Models', link: 'tutorials/body-nested-models/' },
            { label: 'Extra Data Types', link: 'tutorials/extra-data-types/' },
            { label: 'Cookie Parameters', link: 'tutorials/cookie-params/' },
            { label: 'Header Parameters', link: 'tutorials/header-params/' },
            { label: 'Response Model — Return Type', link: 'tutorials/response-model/' },
            { label: 'Extra Models', link: 'tutorials/extra-models/' },
            { label: 'Response Status Code', link: 'tutorials/response-status-code/' },
            { label: 'Form Data', link: 'tutorials/form-data/' },
            { label: 'Form Models', link: 'tutorials/form-models/' },
            { label: 'Request Files', link: 'tutorials/request-files/' },
            { label: 'Request Forms & Files', link: 'tutorials/request-forms-files/' },
            { label: 'Handling Errors', link: 'tutorials/error-handling/' },
            { label: 'Path Operation Configuration', link: 'tutorials/path-operation-config/' },
            { label: 'JSON Compatible Encoder', link: 'tutorials/encoder/' },
            { label: 'Body — Updates', link: 'tutorials/body-updates/' },
            {
              label: 'Dependencies',
              collapsed: true,
              items: [
                { label: 'Classes as Dependencies', link: 'tutorials/dependencies/classes-as-dependencies/' },
                { label: 'Sub-dependencies', link: 'tutorials/dependencies/sub-dependencies/' },
                { label: 'Dependencies in Path Ops', link: 'tutorials/dependencies/dependencies-in-path-ops/' },
                { label: 'Global Dependencies', link: 'tutorials/dependencies/global-dependencies/' },
                { label: 'Dependencies with yield', link: 'tutorials/dependencies/dependencies-with-yield/' },
              ],
            },
            {
              label: 'Security',
              collapsed: true,
              items: [
                { label: 'Security — First Steps', link: 'tutorials/security/first-steps/' },
                { label: 'Get Current User', link: 'tutorials/security/get-current-user/' },
                { label: 'Simple OAuth2', link: 'tutorials/security/simple-oauth2/' },
                { label: 'OAuth2 + JWT Tokens', link: 'tutorials/security/oauth2-jwt/' },
              ],
            },
            { label: 'Middleware', link: 'tutorials/middleware/' },
            { label: 'CORS (Cross-Origin Resource Sharing)', link: 'tutorials/cors/' },
            { label: 'SQL Databases', link: 'tutorials/database-integration/' },
            { label: 'Bigger Applications — Multiple Files', link: 'tutorials/routing-subrouters/' },
            { label: 'Background Tasks', link: 'tutorials/background-tasks/' },
            { label: 'Metadata & Docs URLs', link: 'tutorials/metadata/' },
            { label: 'Static Files', link: 'tutorials/static-files/' },
            { label: 'Scalar API Reference', link: 'tutorials/scalar-ui/' },
            { label: 'Testing', link: 'tutorials/testing/' },
            { label: 'Debugging', link: 'tutorials/debugging/' },
          ],
        },
        {
          label: 'Advanced User Guide',
          items: [
            { label: 'Zero-GIL Architecture', link: 'advanced/zero-gil-architecture/' },
            { label: 'Rust Core Deep Dive', link: 'advanced/rust-core-deep-dive/' },
            { label: 'PyO3 & FFI Safety', link: 'advanced/pyo3-ffi-safety/' },
            { label: 'Native Fast Path', link: 'advanced/native-fast-path/' },
            { label: 'Multi-Protocol APIs', link: 'advanced/multi-protocol-apis/' },
            { label: 'Agent System', link: 'advanced/agent-system/' },
            { label: 'Streaming Output', link: 'advanced/streaming-output/' },
            { label: 'Performance Tuning', link: 'advanced/performance-tuning/' },
            { label: 'Resilience Patterns', link: 'advanced/resilience-patterns/' },
            { label: 'Return a Response Directly', link: 'advanced/response-directly/' },
            { label: 'Custom Response Classes', link: 'advanced/custom-response/' },
            { label: 'Additional Responses in OpenAPI', link: 'advanced/additional-responses/' },
            { label: 'Response Cookies & Headers', link: 'advanced/response-cookies-headers/' },
            { label: 'Advanced Dependencies', link: 'advanced/advanced-dependencies/' },
            { label: 'Advanced Security', link: 'advanced/advanced-security/' },
            { label: 'Using the Request Directly', link: 'advanced/using-request-directly/' },
            { label: 'Using Dataclasses', link: 'advanced/dataclasses/' },
            { label: 'Advanced Middleware', link: 'advanced/advanced-middleware/' },
            { label: 'Sub Applications — Mounts', link: 'advanced/sub-applications/' },
            { label: 'Behind a Proxy', link: 'advanced/behind-a-proxy/' },
            { label: 'Templates (Jinja2)', link: 'advanced/templates/' },
            { label: 'WebSockets', link: 'advanced/websockets-advanced/' },
            { label: 'Lifespan Events', link: 'advanced/lifespan-events/' },
            { label: 'Async Tests', link: 'advanced/async-tests/' },
            { label: 'Settings & Environment Variables', link: 'advanced/settings/' },
            { label: 'OpenAPI Callbacks & Webhooks', link: 'advanced/openapi-callbacks/' },
            { label: 'Generating SDKs', link: 'advanced/generate-clients/' },
          ],
        },
        {
          label: 'How-To Recipes',
          items: [
            { label: 'Overview', link: 'how-to/' },
            { label: 'Troubleshooting', link: 'how-to/troubleshooting/' },
            { label: 'Performance Tuning', link: 'how-to/performance-tuning/' },
            { label: 'GraphQL Integration', link: 'how-to/graphql/' },
            { label: 'Custom Request & Route Classes', link: 'how-to/custom-request-route/' },
            { label: 'Configure Swagger UI', link: 'how-to/configure-swagger-ui/' },
            { label: 'Testing a Database', link: 'how-to/testing-database/' },
            { label: 'Circuit Breaker Recipes', link: 'how-to/circuit-breaker-recipes/' },
            { label: 'Background Task Patterns', link: 'how-to/background-task-patterns/' },
          ],
        },
        {
          label: 'API Reference',
          items: [
            { label: 'Overview', link: 'api-reference/' },
            { label: 'JustAPIApp', link: 'api-reference/justapiapp/' },
            { label: 'Routing', link: 'api-reference/routing/' },
            { label: 'APIRouter', link: 'api-reference/apirouter/' },
            { label: 'Request Object', link: 'api-reference/request/' },
            { label: 'Response Classes', link: 'api-reference/responses/' },
            { label: 'Dependency Injection', link: 'api-reference/dependency-injection/' },
            { label: 'Exceptions & Errors', link: 'api-reference/exceptions/' },
            { label: 'Schema & Validation', link: 'api-reference/schema-validation/' },
            { label: 'WebSockets', link: 'api-reference/websockets/' },
            { label: 'Background Tasks', link: 'api-reference/background-tasks/' },
            { label: 'Scheduler', link: 'api-reference/scheduler/' },
            { label: 'Session (Agent)', link: 'api-reference/session/' },
            { label: 'Plugin System', link: 'api-reference/plugins/' },
            { label: 'Testing Client', link: 'api-reference/testing-client/' },
            { label: 'UploadFile', link: 'api-reference/uploadfile/' },
            { label: 'Database', link: 'api-reference/database/' },
          ],
        },
        {
          label: 'Deployment',
          items: [
            { label: 'Docker & Docker Compose', link: 'deployment/docker/' },
            { label: 'Kubernetes / Helm', link: 'deployment/kubernetes-helm/' },
            { label: 'Cloudflare Pages', link: 'deployment/cloudflare-pages/' },
            { label: 'Google Cloud (GKE)', link: 'deployment/gke/' },
            { label: 'Amazon EKS', link: 'deployment/eks/' },
            { label: 'Azure AKS', link: 'deployment/aks/' },
            { label: 'Fly.io', link: 'deployment/flyio/' },
            { label: 'Railway', link: 'deployment/railway/' },
            { label: 'Production Checklist', link: 'deployment/production-checklist/' },
          ],
        },
        {
          label: 'Security',
          items: [
            { label: 'Security Policy', link: 'security/policy/' },
            { label: 'OWASP Compliance', link: 'security/owasp-compliance/' },
            { label: 'Penetration Testing', link: 'security/penetration-testing/' },
            { label: 'Secure Configuration', link: 'security/secure-configuration/' },
          ],
        },
        {
          label: 'Observability',
          items: [
            { label: 'Metrics & Monitoring', link: 'observability/metrics-monitoring/' },
            { label: 'OpenTelemetry', link: 'observability/opentelemetry/' },
            { label: 'Structured Logging', link: 'observability/structured-logging/' },
            { label: 'Health Checks', link: 'observability/health-checks/' },
          ],
        },
        {
          label: 'Inference',
          items: [
            { label: 'Overview', link: 'inference/overview/' },
            { label: 'LLM Serving API', link: 'inference/llm-serving-api/' },
            { label: 'GPU & CUDA Setup', link: 'inference/gpu-cuda-setup/' },
            { label: 'Scheduling & Batching', link: 'inference/scheduling-batching/' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'CLI Reference', link: 'reference/cli/' },
            { label: 'Configuration', link: 'reference/configuration/' },
            { label: 'API Stability', link: 'reference/api-stability/' },
            { label: 'Release Notes', link: 'reference/release-notes/' },
            { label: 'ADR Index', link: 'reference/adr-index/' },
            { label: 'Error Codes', link: 'reference/error-codes/' },
            { label: 'Glossary', link: 'reference/glossary/' },
          ],
        },
        {
          label: 'Resources',
          items: [
            { label: 'Help & Support', link: 'resources/help/' },
            { label: 'External Links & Ecosystem', link: 'resources/external-links/' },
            { label: 'About JustAPI', link: 'resources/about/' },
          ],
        },
        {
          label: 'Contributing',
          items: [
            { label: 'Development Setup', link: 'contributing/development-setup/' },
            { label: 'Coding Standards', link: 'contributing/coding-standards/' },
            { label: 'Testing Guide', link: 'contributing/testing-guide/' },
            { label: 'Benchmarking Guide', link: 'contributing/benchmarking-guide/' },
            { label: 'Documentation Guide', link: 'contributing/documentation-guide/' },
          ],
        },
        {
          label: 'Examples',
          link: 'examples/',
        },
      ],
    }),
    sitemap({
      changefreq: 'weekly',
      priority: 0.7,
      lastmod: new Date(),
    }),
  ],
});
