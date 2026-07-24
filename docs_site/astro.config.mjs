import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import sitemap from '@astrojs/sitemap';

export default defineConfig({
  site: 'https://justapi.dev',
  integrations: [
    starlight({
      title: 'JustAPI',
      description: 'Rust-powered, FastAPI-class web framework for Python. 700k+ RPS, zero-GIL execution.',
      logo: {
        src: './src/assets/logo.svg',
        replacesTitle: false,
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/swadhinbiswas/JustAPI' },
      ],
      customCss: ['./src/styles/custom.css'],
      editLink: {
        baseUrl: 'https://github.com/swadhinbiswas/JustAPI/edit/main/docs_site',
      },
      head: [
        {
          tag: 'meta',
          attrs: { property: 'og:title', content: 'JustAPI — Rust-Powered Python Web Framework' },
        },
        {
          tag: 'meta',
          attrs: { property: 'og:description', content: 'The speed of Rust. The elegance of Python. A drop-in replacement for FastAPI engineered for extreme throughput.' },
        },
        {
          tag: 'meta',
          attrs: { property: 'og:image', content: 'https://justapi.dev/og-image.png' },
        },
        {
          tag: 'meta',
          attrs: { name: 'twitter:card', content: 'summary_large_image' },
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
          ],
        },
        {
          label: 'Tutorials',
          items: [
            { label: 'Hello World', link: 'tutorials/hello-world/' },
            { label: 'Path & Query Parameters', link: 'tutorials/path-query-params/' },
            { label: 'Request Body & Validation', link: 'tutorials/request-body/' },
            { label: 'Dependency Injection', link: 'tutorials/dependency-injection/' },
            { label: 'Middleware', link: 'tutorials/middleware/' },
            { label: 'Error Handling', link: 'tutorials/error-handling/' },
            { label: 'File Uploads', link: 'tutorials/file-uploads/' },
            { label: 'WebSockets & SSE', link: 'tutorials/websockets-sse/' },
            { label: 'Database Integration', link: 'tutorials/database-integration/' },
            { label: 'Background Tasks', link: 'tutorials/background-tasks/' },
            { label: 'Routing & Sub-routers', link: 'tutorials/routing-subrouters/' },
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
          label: 'Advanced Guides',
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
          label: 'Reference',
          items: [
            { label: 'CLI Reference', link: 'reference/cli/' },
            { label: 'Configuration', link: 'reference/configuration/' },
            { label: 'Release Notes', link: 'reference/release-notes/' },
            { label: 'ADR Index', link: 'reference/adr-index/' },
            { label: 'Error Codes', link: 'reference/error-codes/' },
            { label: 'Glossary', link: 'reference/glossary/' },
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
    sitemap(),
  ],
});
