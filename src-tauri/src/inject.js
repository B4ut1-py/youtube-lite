// Se ejecuta en cada página cargada en la ventana. Solo actúa en YouTube Music y en Spotify.
(() => {
  const SP = location.hostname === 'open.spotify.com';
  // El inicio de sesión con Google/Spotify abre una ventana emergente: aquí se hace en la misma ventana.
  if (/^(accounts\.(spotify|google)\.com|open\.spotify\.com)$/.test(location.hostname)) {
    const open0 = window.open;
    window.open = (u, ...r) => { if (u && /^https:\/\/(accounts\.(spotify|google)\.com|open\.spotify\.com)\//.test(String(u))) { location.href = u; return null; } return open0.call(window, u, ...r); };
  }
  if (location.hostname !== 'music.youtube.com' && !SP) return;

  // Única modificación visual: con la ventana oculta se pausan animaciones para gastar menos CPU.
  const css = `
    html.lm-hidden *, html.lm-hidden *::before, html.lm-hidden *::after { animation: none !important; transition: none !important; }
    html.lm-hidden ytmusic-player-page, html.lm-hidden #player { visibility: hidden !important; }
    /* Modo ahorro: sin dibujar nada de la interfaz (el audio, el <video> y los tiempos siguen funcionando) */
    html.lm-full body { display: none !important; }
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
  // Tiempos de la canción tal como los muestra YouTube Music (su barra y su "0:12 / 3:45"). El elemento <video>
  // puede durar distinto que la canción, y entonces la barra nunca llega al final.
  const num = (x) => { x = Number(x); return Number.isFinite(x) ? x : 0; };
  const toSec = (t) => t.split(':').reduce((a, b) => a * 60 + Number(b), 0);
  const readTimes = (v) => {
    const sl = document.querySelector('ytmusic-player-bar #progress-bar');
    const m = (document.querySelector('ytmusic-player-bar .time-info')?.textContent || '').match(/(\d+(?::\d+)*)\s*\/\s*(\d+(?::\d+)*)/);
    if (m && toSec(m[2]) > 0) {                                  // el texto "0:12 / 3:45" manda: son segundos seguros
      const dur = toSec(m[2]);
      const exact = sl && Math.abs(num(sl.max) - dur) < 1.5;     // la barra, si usa la misma escala, tiene más precisión
      return { pos: exact ? num(sl.value) : toSec(m[1]), dur };
    }
    if (sl && num(sl.max) > 0 && num(sl.max) !== 100) return { pos: num(sl.value), dur: num(sl.max) };
    return { pos: num(v?.currentTime), dur: num(v?.duration) };
  };
  const click = (sel) => document.querySelector(sel)?.click();

  // ---- Spotify: se lee el reproductor de la página (barra inferior) y se pulsan sus botones ----
  // Spotify necesita Widevine: si no está disponible se avisa una vez (en vez de que la música simplemente no arranque).
  if (SP && !sessionStorage.getItem('lm-drm-checked')) {
    sessionStorage.setItem('lm-drm-checked', '1');
    const cfg = [{ initDataTypes: ['cenc'], audioCapabilities: [{ contentType: 'audio/mp4; codecs="mp4a.40.2"', robustness: 'SW_SECURE_CRYPTO' }] }];
    setTimeout(() => navigator.requestMediaKeySystemAccess('com.widevine.alpha', cfg).catch((e) =>
      alert('Lite Music: Widevine no está disponible en este equipo, por eso Spotify no reproduce.\n' + e)), 4000);
  }
  const T = (id) => `[data-testid="${id}"]`;
  // El audio de Spotify no está en la página (el único <video> visible es el "Canvas": un clip mudo que se repite).
  // Se toma el elemento que suena cuando Spotify lo reproduce; con la ventana oculta el Canvas no se reproduce (ahorra CPU).
  let spAudio = null, lmHidden = false;
  if (SP) {
    const play0 = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = function (...a) {
      if (this.muted && lmHidden) return Promise.resolve();
      if (!this.muted) spAudio = this;
      return play0.apply(this, a);
    };
  }
  const spMedia = () => spAudio;
  let spPrev = { pos: -1, at: 0, playing: false };
  const qs = (sel, root = document) => root.querySelector(sel);
  // Se prueba cada fuente en orden; la interfaz de Spotify cambia a menudo, así que ninguna es imprescindible.
  const spState = () => {
    const md = navigator.mediaSession?.metadata;
    const w = qs(T('now-playing-widget')) || qs(T('now-playing-bar')) || qs('footer');
    const fromTitle = (document.title || '').split(' • ');                // "Canción • Artista" mientras suena
    const title = md?.title || qs(T('context-item-link'), w || document)?.textContent || qs('a[href*="/track/"]', w || document)?.textContent
      || (fromTitle.length > 1 ? fromTitle[0] : '');
    const artists = [...(w || document).querySelectorAll('a[href*="/artist/"]')].map((a) => a.textContent).filter(Boolean);
    const artist = md?.artist || [...new Set(artists)].join(', ') || (fromTitle.length > 1 ? fromTitle.slice(1).join(' • ') : '');
    const art = md?.artwork || [];
    const px = (a) => parseInt((a.sizes || '0').split('x')[0], 10) || 0;
    const cover = (art.length ? art.reduce((b, a) => (px(a) >= px(b) ? a : b)).src : '') || qs('img[src*="scdn.co"]', w || document)?.src || '';
    // tiempos: textos "1:05" / "3:30" de la barra; si no están, la barra deslizante; si no, el elemento de audio
    const tx = (id) => (qs(T(id))?.textContent || '').trim();
    let pos = /^\d+(:\d+)*$/.test(tx('playback-position')) ? toSec(tx('playback-position')) : -1;
    let dur = /^\d+(:\d+)*$/.test(tx('playback-duration')) ? toSec(tx('playback-duration')) : -1;
    const m = spMedia();
    if (dur < 0) {
      const sl = qs('footer input[type=range][max]', document);
      const sm = sl && num(sl.max) > 1000 ? 1000 : 1;                       // en milisegundos o en segundos
      if (sl && num(sl.max) > 100) { dur = num(sl.max) / sm; pos = num(sl.value) / sm; }
      else if (m && Number.isFinite(m.duration) && m.duration > 0) { dur = m.duration; pos = m.currentTime; }
    }
    pos = Math.max(0, pos); dur = Math.max(0, dur);
    const ps = navigator.mediaSession?.playbackState;
    const now = Date.now();
    let playing;
    if (m && m.readyState > 0) playing = !m.paused && !m.ended;
    else if (ps === 'playing' || ps === 'paused') playing = ps === 'playing';
    else {                                                     // sin dato fiable: si el tiempo avanza, suena
      if (pos !== spPrev.pos) { spPrev = { pos, at: now, playing: pos > spPrev.pos && spPrev.pos >= 0 }; }
      playing = spPrev.playing && now - spPrev.at < 1500;
    }
    return { title, artist, cover, playing, pos, dur };
  };
  const spSeek = (f) => {
    const bar = document.querySelector(T('playback-progressbar'));
    if (!bar) return;
    const r = bar.getBoundingClientRect(); const x = r.left + r.width * f, y = r.top + r.height / 2;
    ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click'].forEach((t) =>
      bar.dispatchEvent(new MouseEvent(t, { bubbles: true, clientX: x, clientY: y, view: window })));
  };

  window.__lm = SP ? {
    playPause: () => click(T('control-button-playpause')),
    next: () => click(T('control-button-skip-forward')),
    prev: () => click(T('control-button-skip-back')),
    setVolume: (f) => { const m = spMedia(); if (m) m.volume = Math.min(1, Math.max(0, f)); },
    play: () => { if (!spState().playing) click(T('control-button-playpause')); },
    pause: () => { if (spState().playing) click(T('control-button-playpause')); },
    times: () => { const s = spState(); return { pos: s.pos, dur: s.dur }; },
    seek: (f) => spSeek(f),
    setHidden: (h, full) => {
      lmHidden = h;
      document.querySelectorAll('video').forEach((v) => { if (v.muted) h ? v.pause() : v.play().catch(() => {}); });
      const r = document.documentElement; if (!r) return; r.classList.toggle('lm-hidden', h); r.classList.toggle('lm-full', !!full);
    },
  } : {
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
    play: () => { const v = document.querySelector('video'); if (v && v.paused) v.play(); },
    pause: () => { const v = document.querySelector('video'); if (v && !v.paused) v.pause(); },
    times: () => readTimes(document.querySelector('video')),
    seek: (f) => { const v = document.querySelector('video'); const d = readTimes(v).dur; if (v && d) v.currentTime = f * d; },
    setHidden: (h, full) => {
      const r = document.documentElement; if (!r) return;
      r.classList.toggle('lm-hidden', h); r.classList.toggle('lm-full', !!full);
    },
  };

  // Informa de la canción al reproductor flotante. Solo se envía cuando algo CAMBIA (canción, pausa, duración, volumen,
  // un salto en el tiempo) o como señal de vida cada 30 s; entre informes el flotante calcula el tiempo por su cuenta.
  // Revisar la página es barato (cada 2 s si el flotante se ve, 5 s si no); enviar mensajes es lo que cuesta memoria.
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (!invoke) return;
  let wanted = true, timer = 0, sent = null, sentAt = 0;
  const snapshot = () => {
    if (SP) { const m = spMedia(); return { vol: m ? m.volume : 1, ...spState() }; }
    const md = navigator.mediaSession?.metadata;
    const v = document.querySelector('video'); if (v) applyVol(v);
    const art = md?.artwork || [];
    const px = (a) => parseInt((a.sizes || '0').split('x')[0], 10) || 0;
    const cover = art.length ? art.reduce((b, a) => (px(a) >= px(b) ? a : b)).src : '';
    return { title: md?.title || '', artist: md?.artist || '', cover, vol: v ? v.volume : 1, playing: !!v && !v.paused && !v.ended, ...readTimes(v) };
  };
  const changed = (n) => {
    if (!sent) return true;
    if (n.title !== sent.title || n.artist !== sent.artist || n.cover !== sent.cover || n.playing !== sent.playing) return true;
    if (Math.abs(n.dur - sent.dur) > 1 || Math.abs(n.vol - sent.vol) > 0.01) return true;
    const now = Date.now();
    const expected = sent.pos + (n.playing ? (now - sentAt) / 1000 : 0);
    return Math.abs(n.pos - expected) > 3 || now - sentAt > 30000;          // salto de tiempo o señal de vida
  };
  const tick = async () => {
    clearTimeout(timer);
    try {
      const n = snapshot();
      if (changed(n)) { sent = n; sentAt = Date.now(); wanted = await invoke('report_state', n); }
    } catch (_) {}
    timer = setTimeout(tick, wanted ? 2000 : 5000);
  };
  // Cambios inmediatos: eventos del audio/vídeo y órdenes desde el flotante o la bandeja.
  const kick = (ms = 250) => { clearTimeout(timer); timer = setTimeout(tick, ms); };
  ['play', 'pause', 'ended', 'loadedmetadata', 'emptied'].forEach((ev) => document.addEventListener(ev, () => kick(), true));
  window.__lm.kick = kick;
  tick();
})();
