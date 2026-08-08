// JustAPI docs — animations (anime.js v4)
// Count-up stats, scroll-reveal, hero entrance, benchmark bars, terminal.
import { animate, createTimeline, utils } from 'animejs';

export function initAnimations() {
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (prefersReduced) return;

  // ─── Animated stat counters (data-count attributes) ───
  document.querySelectorAll('[data-count]').forEach((el) => {
    const target = parseFloat(el.dataset.count);
    const suffix = el.dataset.suffix || '';
    const decimals = el.dataset.decimals ? parseInt(el.dataset.decimals, 10) : 0;
    const obj = { v: 0 };
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return;
        io.disconnect();
        animate({
          targets: obj,
          v: target,
          duration: 1400,
          ease: 'outExpo',
          update: () => {
            el.textContent = obj.v.toFixed(decimals) + suffix;
          },
        });
      },
      { threshold: 0.4 },
    );
    io.observe(el);
  });

  // ─── Scroll-reveal sections ───
  document.querySelectorAll('[data-reveal]').forEach((el) => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(24px)';
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return;
        io.disconnect();
        animate({
          targets: el,
          opacity: [0, 1],
          translateY: [24, 0],
          duration: 700,
          ease: 'outCubic',
          delay: parseInt(el.dataset.revealDelay || '0', 10),
        });
      },
      { threshold: 0.15 },
    );
    io.observe(el);
  });

  // ─── Staggered children reveal ───
  document.querySelectorAll('[data-reveal-group]').forEach((group) => {
    const items = Array.from(group.querySelectorAll('[data-reveal-item]'));
    items.forEach((it) => {
      it.style.opacity = '0';
      it.style.transform = 'translateY(20px)';
    });
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return;
        io.disconnect();
        animate({
          targets: items,
          opacity: [0, 1],
          translateY: [20, 0],
          duration: 650,
          delay: (el, i) => i * 90,
          ease: 'outCubic',
        });
      },
      { threshold: 0.12 },
    );
    io.observe(group);
  });

  // ─── Hero entrance ───
  const heroTitle = document.querySelector('[data-hero-title]');
  if (heroTitle) {
    createTimeline({ ease: 'outExpo' })
      .add({
        targets: heroTitle,
        opacity: [0, 1],
        translateY: [30, 0],
        duration: 800,
      })
      .add({
        targets: heroTitle,
        textShadow: ['0 0 0px rgba(34,197,94,0)', '0 0 40px rgba(34,197,94,0.35)'],
        duration: 900,
      }, '-=400');
  }

  // ─── Terminal typing reveal ───
  const term = document.querySelector('[data-terminal]');
  if (term) {
    const lines = Array.from(term.querySelectorAll('[data-term-line]'));
    lines.forEach((l) => { l.style.opacity = '0'; });
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return;
        io.disconnect();
        createTimeline({ ease: 'outCubic' })
          .add({
            targets: lines,
            opacity: [0, 1],
            translateX: [-8, 0],
            duration: 220,
            delay: (el, i) => i * 140,
          });
      },
      { threshold: 0.3 },
    );
    io.observe(term);
  }

  // ─── Cursor blink ───
  const cursor = document.querySelector('[data-cursor]');
  if (cursor) {
    animate({
      targets: cursor,
      opacity: [1, 0],
      duration: 500,
      loop: true,
      ease: 'steps(1)',
    });
  }

  // ─── Benchmark bars ───
  document.querySelectorAll('[data-bar]').forEach((bar) => {
    const w = bar.dataset.w || '0%';
    bar.style.width = '0%';
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return;
        io.disconnect();
        animate({
          targets: bar,
          width: w,
          duration: 1200,
          ease: 'outExpo',
          delay: parseInt(bar.dataset.delay || '0', 10),
        });
      },
      { threshold: 0.4 },
    );
    io.observe(bar);
  });

  // ─── Tabs (shadcn-style) ───
  document.querySelectorAll('[data-tabs]').forEach((tabs) => {
    const btns = tabs.querySelectorAll('[data-tab-btn]');
    const panels = tabs.querySelectorAll('[data-tab-panel]');
    btns.forEach((btn) => {
      btn.addEventListener('click', () => {
        const target = btn.dataset.tabBtn;
        btns.forEach((b) => b.setAttribute('data-active', b === btn ? 'true' : 'false'));
        panels.forEach((p) => {
          const show = p.dataset.tabPanel === target;
          p.style.display = show ? 'block' : 'none';
          if (show) {
            animate({ targets: p, opacity: [0, 1], translateY: [6, 0], duration: 300, ease: 'outCubic' });
          }
        });
      });
    });
    const first = tabs.querySelector('[data-tab-btn]');
    if (first) first.click();
  });
}

if (typeof window !== 'undefined') {
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initAnimations);
  } else {
    initAnimations();
  }
}
