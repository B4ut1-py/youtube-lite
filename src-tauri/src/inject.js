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
  // Volumen elegido desde el flotante. YouTube lo reinicia solo (cambio de canción, normalización de volumen...),
  // así que se vuelve a aplicar cada vez que cambia. Si tocas el volumen de YouTube Music, manda YouTube.
  let lmVol = null, applying = false, fixes = [];
  const applyVol = (v) => {
    if (lmVol === null || Math.abs(v.volume - lmVol) < 0.005) return;
    const now = Date.now(); fixes = fixes.filter((x) => now - x < 1000); fixes.push(now);
    if (fixes.length > 20) { lmVol = null; return; }               // YouTube lo pelea sin parar: no insistir
    applying = true; v.volume = lmVol; applying = false;
  };
  ['volumechange', 'loadedmetadata', 'loadstart', 'canplay', 'playing'].forEach((ev) =>
    document.addEventListener(ev, (e) => {
      if (!applying && e.target instanceof HTMLVideoElement) applyVol(e.target);
    }, true));
  document.addEventListener('pointerdown', (e) => {
    if (e.target.closest?.('#volume-slider, #expand-volume-slider, .volume-slider')) lmVol = null;
  }, true);
  const click = (sel) => document.querySelector(sel)?.click();
  window.__lm = {
    playPause: () => {
      const v = document.querySelector('video');
      if (v && v.readyState > 0) { v.paused ? v.play() : v.pause(); } else click('.play-pause-button');
    },
    next: () => click('.next-button'),
    prev: () => click('.previous-button'),
    setVolume: (f) => {
      lmVol = Math.min(1, Math.max(0, f));
      const v = document.querySelector('video'); if (v) { applying = true; v.volume = lmVol; applying = false; }
    },
    seek: (f) => { const v = document.querySelector('video'); if (v && v.duration) v.currentTime = f * v.duration; },
    setHidden: (h) => document.documentElement?.classList.toggle('lm-hidden', h),
  };

  // Informa de la canción al reproductor flotante (0,5 s si se ve, 5 s si no)
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (!invoke) return;
  const tick = async () => {
    let wanted = true;
    try {
      const md = navigator.mediaSession?.metadata;
      const v = document.querySelector('video'); if (v) applyVol(v);
      const art = md?.artwork || [];
      const px = (a) => parseInt((a.sizes || '0').split('x')[0], 10) || 0;
      const cover = art.length ? art.reduce((b, a) => (px(a) >= px(b) ? a : b)).src : '';
      wanted = await invoke('report_state', {
        title: md?.title || '', artist: md?.artist || '', cover, vol: v ? v.volume : 1,
        playing: !!v && !v.paused && !v.ended,
        pos: v?.currentTime || 0, dur: Number.isFinite(v?.duration) ? v.duration : 0,
      });
    } catch (_) {}
    setTimeout(tick, wanted ? 500 : 5000);
  };
  tick();
})();
