// Se ejecuta en cada página cargada en la ventana. Solo actúa en YouTube Music.
(() => {
  if (location.hostname !== 'music.youtube.com') return;

  const css = `
    /* Aspecto más limpio */
    ::-webkit-scrollbar { width: 8px; height: 8px; }
    ::-webkit-scrollbar-thumb { background: #ffffff33; border-radius: 8px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ytmusic-mealbar-promo-renderer,
    ytmusic-statement-banner-renderer,
    ytmusic-popup-container tp-yt-paper-dialog:has(ytmusic-mealbar-promo-renderer) { display: none !important; }
    ytmusic-responsive-list-item-renderer, ytmusic-two-row-item-renderer,
    ytmusic-player-bar { border-radius: 10px; }
    img { border-radius: 8px; }

    /* Con la ventana oculta: sin animaciones, menos trabajo de CPU/GPU */
    html.lm-hidden *, html.lm-hidden *::before, html.lm-hidden *::after {
      animation: none !important; transition: none !important;
    }
    html.lm-hidden ytmusic-player-page, html.lm-hidden #player { visibility: hidden !important; }
  `;
  const style = document.createElement('style');
  style.textContent = css;
  (document.head || document.documentElement).appendChild(style);

  // Funciones llamadas desde Rust (bandeja)
  const click = (sel) => document.querySelector(sel)?.click();
  window.__lm = {
    playPause: () => click('.play-pause-button'),
    next: () => click('.next-button'),
    prev: () => click('.previous-button'),
    setHidden: (h) => document.documentElement.classList.toggle('lm-hidden', h),
  };
})();
