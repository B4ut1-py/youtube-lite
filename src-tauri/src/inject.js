// Se ejecuta en cada página cargada en la ventana. Solo actúa en YouTube Music.
(() => {
  if (location.hostname !== 'music.youtube.com') return;

  // Única modificación visual: con la ventana oculta se pausan animaciones para gastar menos CPU.
  const css = `
    html.lm-hidden *, html.lm-hidden *::before, html.lm-hidden *::after { animation: none !important; transition: none !important; }
    html.lm-hidden ytmusic-player-page, html.lm-hidden #player { visibility: hidden !important; }
  `;
  // El script corre al crear el documento: puede que aún no exista <html>.
  const whenRoot = (fn) => {
    if (document.documentElement) return fn();
    const mo = new MutationObserver(() => {
      if (document.documentElement) { mo.disconnect(); fn(); }
    });
    mo.observe(document, { childList: true });
  };
  whenRoot(() => {
    const el = document.createElement('style');
    el.textContent = css;
    (document.head || document.documentElement).appendChild(el);
  });

  // Funciones llamadas desde Rust (bandeja y reproductor flotante)
  const click = (sel) => document.querySelector(sel)?.click();
  window.__lm = {
    playPause: () => {
      const v = document.querySelector('video');
      if (v && v.readyState > 0) { v.paused ? v.play() : v.pause(); } else click('.play-pause-button');
    },
    next: () => click('.next-button'),
    prev: () => click('.previous-button'),
    seek: (f) => { const v = document.querySelector('video'); if (v && v.duration) v.currentTime = f * v.duration; },
    setHidden: (h) => document.documentElement?.classList.toggle('lm-hidden', h),
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
