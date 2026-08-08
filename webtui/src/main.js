// JustAPI Docs — WebTUI Terminal UI Client Controller
(function () {
  'use strict';

  // ─── Theme Switcher System ───
  var THEMES = [
    { id: 'catppuccin', name: 'Catppuccin', icon: '\uf870' },
    { id: 'dark', name: 'Dark', icon: '\uf1db' },
    { id: 'light', name: 'Light', icon: '\uf185' }
  ];

  function applyTheme(t) {
    document.documentElement.setAttribute('data-webtui-theme', t);
    document.documentElement.setAttribute('data-theme', t);
    try { localStorage.setItem('ja-theme', t); } catch (e) {}
    document.querySelectorAll('.ja-theme-btn').forEach(function (b) {
      b.classList.toggle('active', b.dataset.theme === t);
    });
  }

  function initThemeSwitcher() {
    var saved = null;
    try { saved = localStorage.getItem('ja-theme'); } catch (e) {}
    var container = document.getElementById('themeSwitcher');
    if (!container) return;
    container.innerHTML = '';
    
    THEMES.forEach(function (t) {
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'ja-theme-btn';
      btn.dataset.theme = t.id;
      btn.title = 'Switch theme to ' + t.name;
      btn.setAttribute('aria-label', 'Theme: ' + t.name);
      btn.innerHTML = '<span class="nf">' + t.icon + '</span> ' + t.name;
      btn.addEventListener('click', function () { applyTheme(t.id); });
      container.appendChild(btn);
    });
    
    var validTheme = THEMES.some(function (t) { return t.id === saved; });
    applyTheme(validTheme ? saved : 'catppuccin');
  }

  // ─── Quick Navigation Pages Index for Search Modal ───
  var SEARCH_PAGES = [
    { title: "Overview & Philosophy", url: "/getting-started/overview/" },
    { title: "Installation", url: "/getting-started/installation/" },
    { title: "First Steps", url: "/getting-started/first-steps/" },
    { title: "CLI Scaffolder", url: "/getting-started/cli-scaffolder/" },
    { title: "Migrate from FastAPI", url: "/getting-started/migrating-from-fastapi/" },
    { title: "Hello World Tutorial", url: "/tutorials/hello-world/" },
    { title: "Path Parameters", url: "/tutorials/path-params/" },
    { title: "Query Parameters", url: "/tutorials/query-params/" },
    { title: "Request Body", url: "/tutorials/request-body/" },
    { title: "Error Handling", url: "/tutorials/error-handling/" },
    { title: "Dependency Injection", url: "/tutorials/dependency-injection/" },
    { title: "Middleware", url: "/tutorials/middleware/" },
    { title: "CORS", url: "/tutorials/cors/" },
    { title: "Database Integration", url: "/tutorials/database-integration/" },
    { title: "Background Tasks", url: "/tutorials/background-tasks/" },
    { title: "Testing", url: "/tutorials/testing/" },
    { title: "Static Files", url: "/tutorials/static-files/" },
    { title: "Zero-GIL Architecture", url: "/advanced/zero-gil-architecture/" },
    { title: "Rust Core Deep Dive", url: "/advanced/rust-core-deep-dive/" },
    { title: "Native Fast Path", url: "/advanced/native-fast-path/" },
    { title: "Streaming Output", url: "/advanced/streaming-output/" },
    { title: "WebSockets Advanced", url: "/advanced/websockets-advanced/" },
    { title: "Templates", url: "/advanced/templates/" },
    { title: "Performance Tuning", url: "/advanced/performance-tuning/" },
    { title: "Resilience Patterns", url: "/advanced/resilience-patterns/" },
    { title: "JustAPIApp API Reference", url: "/api-reference/justapiapp/" },
    { title: "Routing API Reference", url: "/api-reference/routing/" },
    { title: "Request API Reference", url: "/api-reference/request/" },
    { title: "Responses API Reference", url: "/api-reference/responses/" },
    { title: "Dependency Injection API Reference", url: "/api-reference/dependency-injection/" },
    { title: "Exceptions API Reference", url: "/api-reference/exceptions/" },
    { title: "Schema Validation API Reference", url: "/api-reference/schema-validation/" },
    { title: "WebSockets API Reference", url: "/api-reference/websockets/" },
    { title: "Background Tasks API Reference", url: "/api-reference/background-tasks/" },
    { title: "Scheduler API Reference", url: "/api-reference/scheduler/" },
    { title: "Database API Reference", url: "/api-reference/database/" },
    { title: "Testing Client API Reference", url: "/api-reference/testing-client/" },
    { title: "UploadFile API Reference", url: "/api-reference/uploadfile/" },
    { title: "Docker Deployment", url: "/deployment/docker/" },
    { title: "Kubernetes & Helm Deployment", url: "/deployment/kubernetes-helm/" },
    { title: "Cloudflare Pages Deployment", url: "/deployment/cloudflare-pages/" },
    { title: "Production Checklist", url: "/deployment/production-checklist/" },
    { title: "Security Policy", url: "/security/policy/" },
    { title: "OWASP Compliance", url: "/security/owasp-compliance/" },
    { title: "Secure Configuration", url: "/security/secure-configuration/" },
    { title: "Metrics & Monitoring", url: "/observability/metrics-monitoring/" },
    { title: "OpenTelemetry Tracing", url: "/observability/opentelemetry/" },
    { title: "Structured Logging", url: "/observability/structured-logging/" },
    { title: "Health Checks", url: "/observability/health-checks/" },
    { title: "CLI Command Reference", url: "/reference/cli/" },
    { title: "Configuration Reference", url: "/reference/configuration/" },
    { title: "API Stability", url: "/reference/api-stability/" },
    { title: "Release Notes", url: "/reference/release-notes/" },
    { title: "Glossary", url: "/reference/glossary/" }
  ];

  // ─── Search Dialog Controller ───
  function initSearchModal() {
    var dialog = document.getElementById('search-dialog');
    var openBtn = document.getElementById('openSearchBtn');
    var closeBtn = document.getElementById('closeSearchBtn');
    var input = document.getElementById('searchInput');
    var results = document.getElementById('searchResults');

    if (!dialog || !openBtn || !input || !results) return;

    function renderResults(query) {
      results.innerHTML = '';
      var q = (query || '').toLowerCase().trim();
      var filtered = SEARCH_PAGES.filter(function (p) {
        return !q || p.title.toLowerCase().indexOf(q) !== -1 || p.url.toLowerCase().indexOf(q) !== -1;
      });

      if (!filtered.length) {
        results.innerHTML = '<div style="padding: 1rem; color: var(--foreground2);">No matching documentation pages found</div>';
        return;
      }

      filtered.forEach(function (p, idx) {
        var a = document.createElement('a');
        a.className = 'ja-search-item' + (idx === 0 ? ' active' : '');
        a.href = p.url;
        a.innerHTML = '<span style="color: var(--green);">&#xf105;</span> ' + p.title + ' <span style="font-size: 0.75rem; color: var(--foreground2); float: right;">' + p.url + '</span>';
        results.appendChild(a);
      });
    }

    function openModal() {
      dialog.setAttribute('open', '');
      input.value = '';
      renderResults('');
      setTimeout(function () { input.focus(); }, 50);
    }

    function closeModal() {
      dialog.removeAttribute('open');
    }

    openBtn.addEventListener('click', openModal);
    if (closeBtn) closeBtn.addEventListener('click', closeModal);

    dialog.addEventListener('click', function (e) {
      if (e.target === dialog) closeModal();
    });

    input.addEventListener('input', function () {
      renderResults(input.value);
    });

    document.addEventListener('keydown', function (e) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        if (dialog.hasAttribute('open')) closeModal();
        else openModal();
      } else if (e.key === 'Escape' && dialog.hasAttribute('open')) {
        closeModal();
      }
    });
  }

  // ─── Terminal Tabs ───
  function initTabs() {
    document.querySelectorAll('.ja-tabs').forEach(function (tabs) {
      var btns = tabs.querySelectorAll('[data-tab-btn]');
      var panels = tabs.querySelectorAll('[data-tab-panel]');
      btns.forEach(function (btn) {
        btn.addEventListener('click', function () {
          var target = btn.dataset.tabBtn;
          btns.forEach(function (b) { b.setAttribute('data-active', b === btn ? 'true' : 'false'); });
          panels.forEach(function (p) { p.style.display = p.dataset.tabPanel === target ? 'block' : 'none'; });
        });
      });
    });
  }

  // ─── Benchmark Bars Animation ───
  function initBars() {
    var bars = document.querySelectorAll('.ja-bar-fill');
    if (!bars.length || !('IntersectionObserver' in window)) return;
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) {
          e.target.style.width = e.target.getAttribute('data-w') || '0%';
          io.unobserve(e.target);
        }
      });
    }, { threshold: 0.3 });
    bars.forEach(function (b) { io.observe(b); });
  }

  // ─── Sidebar Link Highlight ───
  function initSidebar() {
    var path = window.location.pathname;
    document.querySelectorAll('.ja-nav-tree a').forEach(function (a) {
      var href = a.getAttribute('href');
      if (href === path || (href !== '/' && path.indexOf(href) === 0)) {
        a.classList.add('active');
      }
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () {
      initThemeSwitcher();
      initSearchModal();
      initTabs();
      initBars();
      initSidebar();
    });
  } else {
    initThemeSwitcher();
    initSearchModal();
    initTabs();
    initBars();
    initSidebar();
  }
})();
