// Landing page: terminal tabs + benchmark bars
export function initLanding() {
  document.querySelectorAll('.ja-tabs').forEach((tabs) => {
    const btns = tabs.querySelectorAll('[data-tab-btn]');
    const panels = tabs.querySelectorAll('[data-tab-panel]');
    btns.forEach((btn) => {
      btn.addEventListener('click', () => {
        const target = btn.dataset.tabBtn;
        btns.forEach((b) => b.setAttribute('data-active', b === btn ? 'true' : 'false'));
        panels.forEach((p) => {
          p.style.display = p.dataset.tabPanel === target ? 'block' : 'none';
        });
      });
    });
  });
  const bars = document.querySelectorAll('.ja-bar-fill');
  if (bars.length && 'IntersectionObserver' in window) {
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => {
        if (e.isIntersecting) {
          (e.target as HTMLElement).style.width = e.target.getAttribute('data-w') || '0%';
          io.unobserve(e.target);
        }
      });
    }, { threshold: 0.4 });
    bars.forEach((b) => io.observe(b));
  }
}
