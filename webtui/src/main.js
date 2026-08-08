// JustAPI Docs — terminal UI (pure JS, no framework)
(function () {
  'use strict';

  // ─── Theme switcher ───
  var THEMES = ['dark', 'light', 'catppuccin', 'everforest', 'nord'];
  var ICONS = {
    dark: '\uf1db',       // nf-fa-moon
    light: '\uf185',      // nf-fa-sun
    catppuccin: '\uf870', // nf-md-palette
    everforest: '\uf1a0', // nf-fa-leaf
    nord: '\uf0c8',       // nf-fa-square
  };

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
    var w = document.getElementById('themeSwitcher');
    if (!w) return;
    THEMES.forEach(function (t) {
      var b = document.createElement('button');
      b.className = 'ja-theme-btn';
      b.dataset.theme = t;
      b.title = t;
      b.setAttribute('aria-label', 'Theme: ' + t);
      b.innerHTML = ICONS[t];
      b.addEventListener('click', function () { applyTheme(t); });
      w.appendChild(b);
    });
    applyTheme(saved && THEMES.indexOf(saved) !== -1 ? saved : 'catppuccin');
  }

  // ─── Terminal tabs ───
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

  // ─── Benchmark bars ───
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
    }, { threshold: 0.4 });
    bars.forEach(function (b) { io.observe(b); });
  }

  // ─── Sidebar active link + mobile toggle ───
  function initSidebar() {
    var path = window.location.pathname;
    document.querySelectorAll('.ja-nav-tree a').forEach(function (a) {
      if (a.getAttribute('href') === path) a.classList.add('active');
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () {
      initThemeSwitcher();
      initTabs();
      initBars();
      initSidebar();
    });
  } else {
    initThemeSwitcher();
    initTabs();
    initBars();
    initSidebar();
  }
})();
