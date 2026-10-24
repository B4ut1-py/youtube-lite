// Se ejecuta en cada página cargada en la ventana. Solo actúa en YouTube Music.
(() => {
  if (location.hostname !== 'music.youtube.com') return;

  const css = `
    :root {
      --lm-bg: #0b0b0f; --lm-surface: #15151c; --lm-hover: #1d1d27; --lm-line: #ffffff14;
      --lm-text: #f4f4f7; --lm-dim: #9a9aa8; --lm-accent: #ff2d55;
      --ytmusic-general-background-a: var(--lm-bg) !important;
      --ytmusic-general-background-b: var(--lm-bg) !important;
      --ytmusic-general-background-c: var(--lm-surface) !important;
      --ytmusic-background: var(--lm-bg) !important;
      --ytmusic-nav-bar-background: var(--lm-bg) !important;
      --ytmusic-player-bar-background: var(--lm-surface) !important;
      --ytmusic-search-background: var(--lm-surface) !important;
      --ytmusic-brand-background-solid: var(--lm-bg) !important;
      --ytmusic-brand-background-opaque: var(--lm-bg) !important;
      --ytmusic-color-white1: var(--lm-text) !important;
      --yt-spec-red: var(--lm-accent) !important;
    }
    html, body, ytmusic-app, ytmusic-app-layout { background: var(--lm-bg) !important; }
    ytmusic-app-layout > [slot="main-panel"] { background: var(--lm-bg) !important; }
    #nav-bar-background, #nav-bar-divider { background: var(--lm-bg) !important; border: 0 !important; }
    ytmusic-nav-bar { background: var(--lm-bg) !important; }
    ytmusic-search-box { border-radius: 14px !important; background: var(--lm-surface) !important; border: 1px solid var(--lm-line) !important; }
    #guide-wrapper, ytmusic-guide-renderer, #mini-guide-background, ytmusic-app-layout #guide-content { background: var(--lm-bg) !important; border: 0 !important; }
    ytmusic-guide-entry-renderer tp-yt-paper-item { border-radius: 12px !important; margin: 2px 8px !important; }
    ytmusic-guide-entry-renderer[active] tp-yt-paper-item, ytmusic-guide-entry-renderer tp-yt-paper-item:hover { background: var(--lm-hover) !important; }
    ytmusic-guide-entry-renderer[active] yt-icon, ytmusic-guide-entry-renderer[active] .title { color: var(--lm-accent) !important; fill: var(--lm-accent) !important; }
    ytmusic-guide-section-renderer { border: 0 !important; }

    /* Barra del reproductor: plana, sin degradados */
    ytmusic-player-bar { background: var(--lm-surface) !important; border-top: 1px solid var(--lm-line) !important; }
    #progress-bar.ytmusic-player-bar { --paper-slider-active-color: var(--lm-accent); --paper-slider-knob-color: var(--lm-accent); }
    ytmusic-player-bar .title { font-weight: 600 !important; letter-spacing: .1px; }

    /* Tarjetas y listas */
    ytmusic-two-row-item-renderer, ytmusic-responsive-list-item-renderer, ytmusic-carousel-shelf-renderer { background: transparent !important; }
    ytmusic-responsive-list-item-renderer { border-radius: 12px !important; border: 0 !important; }
    ytmusic-responsive-list-item-renderer:hover { background: var(--lm-hover) !important; }
    ytmusic-two-row-item-renderer .image, ytmusic-two-row-item-renderer img, ytmusic-thumbnail-renderer, ytmusic-thumbnail-renderer img { border-radius: 14px !important; }
    ytmusic-two-row-item-renderer:hover .image { filter: brightness(1.1); }
    ytmusic-chip-cloud-chip-renderer, ytmusic-chip-cloud-chip-renderer a { border-radius: 999px !important; border: 1px solid var(--lm-line) !important; background: var(--lm-surface) !important; }
    ytmusic-chip-cloud-chip-renderer[chip-style*="selected"], ytmusic-chip-cloud-chip-renderer[selected] { background: var(--lm-text) !important; }
    ytmusic-chip-cloud-chip-renderer[selected] * { color: #111 !important; }
    .title, h2, h1 { letter-spacing: -.2px; }
    ytmusic-carousel-shelf-basic-header-renderer .title { font-size: 26px !important; font-weight: 700 !important; }

    /* Fuera lo sobrante */
    ytmusic-mealbar-promo-renderer, ytmusic-statement-banner-renderer,
    ytmusic-popup-container tp-yt-paper-dialog:has(ytmusic-mealbar-promo-renderer) { display: none !important; }

    /* Scrollbar fina */
    ::-webkit-scrollbar { width: 8px; height: 8px; }
    ::-webkit-scrollbar-thumb { background: #ffffff26; border-radius: 8px; }
    ::-webkit-scrollbar-thumb:hover { background: #ffffff40; }
    ::-webkit-scrollbar-track { background: transparent; }

    /* Con la ventana oculta: sin animaciones, menos trabajo de CPU */
    html.lm-hidden *, html.lm-hidden *::before, html.lm-hidden *::after { animation: none !important; transition: none !important; }
    html.lm-hidden ytmusic-player-page, html.lm-hidden #player { visibility: hidden !important; }
  `;
  const style = document.createElement('style');
  style.textContent = css;
  (document.head || document.documentElement).appendChild(style);

  // Funciones llamadas desde Rust (bandeja y reproductor flotante)
  const click = (sel) => document.querySelector(sel)?.click();
  window.__lm = {
    playPause: () => click('.play-pause-button'),
    next: () => click('.next-button'),
    prev: () => click('.previous-button'),
    seek: (f) => { const v = document.querySelector('video'); if (v && v.duration) v.currentTime = f * v.duration; },
    setHidden: (h) => document.documentElement.classList.toggle('lm-hidden', h),
  };

  // Informa de la canción al reproductor flotante (1 s si se ve, 5 s si no)
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (!invoke) return;
  const tick = async () => {
    let wanted = true;
    try {
      const md = navigator.mediaSession?.metadata;
      const v = document.querySelector('video');
      wanted = await invoke('report_state', {
        title: md?.title || '', artist: md?.artist || '',
        playing: !!v && !v.paused && !v.ended,
        pos: v?.currentTime || 0, dur: Number.isFinite(v?.duration) ? v.duration : 0,
      });
    } catch (_) {}
    setTimeout(tick, wanted ? 1000 : 5000);
  };
  tick();
})();
