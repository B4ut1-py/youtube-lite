// Se ejecuta en cada página cargada en la ventana. Solo actúa en YouTube Music y en Spotify.
(() => {
  const SP = location.hostname === 'open.spotify.com';
  // El inicio de sesión con Google/Spotify abre una ventana emergente: aquí se hace en la misma ventana.
  if (/^(accounts\.(spotify|google)\.com|open\.spotify\.com)$/.test(location.hostname)) {
    const open0 = window.open;
    window.open = (u, ...r) => { if (u && /^https:\/\/(accounts\.(spotify|google)\.com|open\.spotify\.com)\//.test(String(u))) { location.href = u; return null; } return open0.call(window, u, ...r); };
  }
  if (location.hostname !== 'music.youtube.com' && !SP) return;

  // Sin el aviso "¿Desea salir del sitio?": la app cambia de servicio o recarga la página sola, y el aviso la frenaba.
  const addEv = EventTarget.prototype.addEventListener;
  EventTarget.prototype.addEventListener = function (t, ...r) { if (t === 'beforeunload' && this === window) return; return addEv.call(this, t, ...r); };
  Object.defineProperty(window, 'onbeforeunload', { get: () => null, set: () => {}, configurable: true });

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

  // Estilo Cristal (opcional, solo apariencia). Rust reemplaza estos marcadores al crear la página
  // y luego lo prende o apaga con __lm.glass() al cambiar el ajuste o al terminar de cargar.
  const GLASS_CSS = /*__LM_GLASS_CSS__*/{}[SP ? 'sp' : 'ytm'] || '';
  // g: null (apagado) o { tone: 'dark' | 'light', tint: '#rrggbb' } con el fondo fijo elegido en Configuración
  let glassEl = null;
  const setGlass = (g) => whenRoot(() => {
    const r = document.documentElement;
    if (!g || !GLASS_CSS) { glassEl?.remove(); glassEl = null; r.classList.remove('lm-glass', 'lm-light'); return; }
    if (!glassEl) { glassEl = document.createElement('style'); glassEl.textContent = GLASS_CSS; }
    (document.head || r).appendChild(glassEl);
    r.classList.add('lm-glass');
    r.classList.toggle('lm-light', g.tone === 'light');
    r.style.setProperty('--lm-tint', g.tint);
  });
  setGlass(/*__LM_GLASS_ON__*/null);
  // YouTube Music con Cristal: la línea de tiempo se dibuja sobre el borde curvo de la cápsula (CSS) con el avance en --lm-p
  let progObs = null;
  const glassProgress = () => {
    const sl = document.querySelector('ytmusic-player-bar #progress-bar');
    if (!sl || progObs?.el === sl) return;
    progObs?.disconnect();
    const put = () => { const max = +sl.getAttribute('aria-valuemax') || 0; document.documentElement.style.setProperty('--lm-p', max ? Math.min(1, (+sl.getAttribute('aria-valuenow') || 0) / max).toFixed(4) : 0); };
    progObs = new MutationObserver(put); progObs.el = sl;
    progObs.observe(sl, { attributes: true, attributeFilter: ['aria-valuenow', 'aria-valuemax'] });
    put();
  };

  // Funciones llamadas desde Rust (bandeja y reproductor flotante)
  // Volumen elegido desde el flotante. YouTube lo reinicia solo (cambio de canción, normalización de volumen...),
  // así que se vuelve a aplicar cada vez que cambia. Si tocas el volumen de YouTube Music, manda YouTube.
  let lmVol = null, applying = false, fixes = [];
  const applyVol = (v) => {
    if (lmVol === null || fadeV !== null || Math.abs(v.volume - lmVol) < 0.005) return;   // durante un fundido no se corrige
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
  // Fundido corto (~250 ms) al pausar o reanudar desde la app; al terminar deja el volumen como estaba.
  // Si llega otra orden a mitad de un fundido, se parte del volumen original (nunca queda más bajo).
  let fadeT = 0, fadeV = null;
  const fade = (m, up, act) => {
    if (!m) return act();
    clearInterval(fadeT);
    if (fadeV === null) fadeV = m.volume;
    const v0 = fadeV, t0 = Date.now(), set = (x) => { applying = true; m.volume = x; applying = false; };
    if (up) { set(0); act(); }
    fadeT = setInterval(() => {
      const k = Math.min(1, (Date.now() - t0) / 250);               // por tiempo: si el navegador lo frena, igual termina
      set(v0 * (up ? k : 1 - k));
      if (k >= 1) { clearInterval(fadeT); fadeV = null; if (!up) { act(); set(v0); } }
    }, 25);
  };

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
  const spLike = () => document.querySelector('[data-testid^="now-playing-widget"] button[aria-checked]');
  const ytLike = () => document.querySelector('ytmusic-player-bar ytmusic-like-button-renderer');
  let spPrev = { pos: -1, at: 0, playing: false };
  const qs = (sel, root = document) => root.querySelector(sel);
  // Se prueba cada fuente en orden; la interfaz de Spotify cambia a menudo, así que ninguna es imprescindible.
  const spState = () => {
    const md = navigator.mediaSession?.metadata;
    const w = qs('[data-testid^="now-playing-widget"]') || qs(T('now-playing-bar')) || qs('footer');
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
    return { title, artist, cover, playing, pos, dur, liked: spLike()?.getAttribute('aria-checked') === 'true' };
  };
  const spSeek = (f) => {
    const bar = document.querySelector(T('playback-progressbar'));
    if (!bar) return;
    const r = bar.getBoundingClientRect(); const x = r.left + r.width * f, y = r.top + r.height / 2;
    ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click'].forEach((t) =>
      bar.dispatchEvent(new MouseEvent(t, { bubbles: true, clientX: x, clientY: y, view: window })));
  };

  window.__lm = SP ? {
    playPause: () => { const on = spState().playing; fade(spMedia(), !on, () => click(T('control-button-playpause'))); },
    next: () => click(T('control-button-skip-forward')),
    prev: () => click(T('control-button-skip-back')),
    setVolume: (f) => { const m = spMedia(); if (m) m.volume = Math.min(1, Math.max(0, f)); },
    play: () => { if (!spState().playing) fade(spMedia(), true, () => click(T('control-button-playpause'))); },
    pause: () => { if (spState().playing) fade(spMedia(), false, () => click(T('control-button-playpause'))); },
    // Si ya está guardada, Spotify no la quita con un clic sino que abre su selector de listas: solo se agrega.
    like: () => { const b = spLike(); if (b && b.getAttribute('aria-checked') !== 'true') b.click(); },
    times: () => { const s = spState(); return { pos: s.pos, dur: s.dur }; },
    seek: (f) => spSeek(f),
    setHidden: (h, full) => {
      lmHidden = h;
      document.querySelectorAll('video').forEach((v) => { if (v.muted) h ? v.pause() : v.play().catch(() => {}); });
      const r = document.documentElement; if (!r) return; r.classList.toggle('lm-hidden', h); r.classList.toggle('lm-full', !!full);
    },
    glass: setGlass,
  } : {
    playPause: () => {
      const v = document.querySelector('video');
      if (v && v.readyState > 0) v.paused ? fade(v, true, () => v.play()) : fade(v, false, () => v.pause());
      else click('.play-pause-button');
    },
    next: () => click('.next-button'),
    prev: () => click('.previous-button'),
    setVolume: (f) => {
      lmVol = Math.min(1, Math.max(0, f));
      const v = document.querySelector('video'); if (v) { applying = true; v.volume = lmVol; applying = false; }
    },
    play: () => { const v = document.querySelector('video'); if (v && v.paused) fade(v, true, () => v.play()); },
    pause: () => { const v = document.querySelector('video'); if (v && !v.paused) fade(v, false, () => v.pause()); },
    like: () => ytLike()?.querySelector('#button-shape-like button')?.click(),
    times: () => readTimes(document.querySelector('video')),
    seek: (f) => { const v = document.querySelector('video'); const d = readTimes(v).dur; if (v && d) v.currentTime = f * d; },
    setHidden: (h, full) => {
      const r = document.documentElement; if (!r) return;
      r.classList.toggle('lm-hidden', h); r.classList.toggle('lm-full', !!full);
    },
    glass: setGlass,
  };

  // Informa de la canción al reproductor flotante. Solo se envía cuando algo CAMBIA (canción, pausa, duración, volumen,
  // un salto en el tiempo) o como señal de vida cada 30 s; entre informes el flotante calcula el tiempo por su cuenta.
  // Revisar la página es barato (cada 2 s si el flotante se ve, 5 s si no); enviar mensajes es lo que cuesta memoria.
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (!invoke) return;
  let wanted = true, timer = 0, sent = null, sentAt = 0;
  const snapshot = () => {
    if (SP) { const m = spMedia(); return { vol: m ? (fadeV ?? m.volume) : 1, ...spState() }; }
    const md = navigator.mediaSession?.metadata;
    const v = document.querySelector('video'); if (v) applyVol(v);
    const art = md?.artwork || [];
    const px = (a) => parseInt((a.sizes || '0').split('x')[0], 10) || 0;
    const cover = art.length ? art.reduce((b, a) => (px(a) >= px(b) ? a : b)).src : '';
    return { title: md?.title || '', artist: md?.artist || '', cover, vol: v ? (fadeV ?? v.volume) : 1, playing: !!v && !v.paused && !v.ended,
      liked: ytLike()?.getAttribute('like-status') === 'LIKE', ...readTimes(v) };
  };
  const changed = (n) => {
    if (!sent) return true;
    if (n.title !== sent.title || n.artist !== sent.artist || n.cover !== sent.cover || n.playing !== sent.playing || n.liked !== sent.liked) return true;
    if (Math.abs(n.dur - sent.dur) > 1 || Math.abs(n.vol - sent.vol) > 0.01) return true;
    const now = Date.now();
    const expected = sent.pos + (n.playing ? (now - sentAt) / 1000 : 0);
    return Math.abs(n.pos - expected) > 3 || now - sentAt > 30000;          // salto de tiempo o señal de vida
  };
  // "¿Sigues ahí?": tras horas sin tocar nada YouTube Music pausa y pregunta; con la ventana cerrada nadie lo ve.
  // Se pulsa su botón solo si el diálogo está abierto (al cerrarlo YouTube Music reanuda por su cuenta).
  const youThere = () => {
    const d = document.querySelector('ytmusic-you-there-renderer')?.closest('tp-yt-paper-dialog');
    if (!d || getComputedStyle(d).display === 'none') return;
    const b = d.querySelector('ytmusic-you-there-renderer yt-button-renderer[dialog-confirm]');
    (b?.querySelector('button') || b)?.click();
  };
  const tick = async () => {
    clearTimeout(timer);
    try {
      if (!SP) { youThere(); glassProgress(); }
      const n = snapshot();
      if (changed(n)) { sent = n; sentAt = Date.now(); wanted = await invoke('report_state', n); }
    } catch (_) {}
    timer = setTimeout(tick, wanted ? 2000 : 5000);
  };
  // Cambios inmediatos: eventos del audio/vídeo y órdenes desde el flotante o la bandeja.
  const kick = (ms = 250) => { clearTimeout(timer); timer = setTimeout(tick, ms); };
  ['play', 'pause', 'ended', 'loadedmetadata', 'emptied'].forEach((ev) => document.addEventListener(ev, () => kick(), true));
  window.__lm.kick = kick;

  // Búsqueda rápida (ventana "search"): devuelve { title, kind, sub, dur, img, round, open } y reproduce por número.
  // YouTube Music usa su API interna de búsqueda; Spotify, su propia página de búsqueda dentro de la app.
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const KINDS = /^(canción|video|vídeo|vídeo musical|artista|álbum|ep|sencillo|single|lista|playlist|episodio|pódcast|podcast|perfil)$/i;
  const YTM_FILTER = { songs: 'EgWKAQIIAWoMEA4QChADEAQQCRAF', albums: 'EgWKAQIYAWoMEA4QChADEAQQCRAF', artists: 'EgWKAQIgAWoMEA4QChADEAQQCRAF', playlists: 'EgeKAQQoAEABagwQDhAKEAMQBBAJEAU=' };
  const KIND_NAME = { songs: 'Canción', albums: 'Álbum', artists: 'Artista', playlists: 'Lista' };
  let found = [];
  const ytmSearch = async (q, kind) => {
    const body = { context: window.ytcfg.get('INNERTUBE_CONTEXT'), query: q };
    if (YTM_FILTER[kind]) body.params = YTM_FILTER[kind];
    const j = await (await fetch('/youtubei/v1/search?prettyPrint=false', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })).json();
    const text = (t) => (t?.runs || []).map((r) => r.text).join('');
    const out = [];
    const walk = (o) => {
      if (!o || typeof o !== 'object' || out.length >= 12) return;
      const m = o.musicResponsiveListItemRenderer;
      if (!m) { for (const k in o) walk(o[k]); return; }
      const cols = (m.flexColumns || []).map((c) => c.musicResponsiveListItemFlexColumnRenderer?.text);
      const fixed = (m.fixedColumns || []).map((c) => text(c.musicResponsiveListItemFixedColumnRenderer?.text));
      const parts = text(cols[1]).split(/\s*•\s*/).filter(Boolean);
      const dur = [...parts, ...fixed].find((p) => /^\d+:\d\d(:\d\d)?$/.test(p)) || '';
      const play = m.overlay?.musicItemThumbnailOverlayRenderer?.content?.musicPlayButtonRenderer?.playNavigationEndpoint || cols[0]?.runs?.[0]?.navigationEndpoint;
      const ep = play?.watchEndpoint || play?.watchPlaylistEndpoint ? play : m.navigationEndpoint;
      if (!ep) return;
      const th = m.thumbnail?.musicThumbnailRenderer;
      const k = KINDS.test(parts[0] || '') ? parts.shift() : KIND_NAME[kind] || '';
      out.push({ title: text(cols[0]), kind: k, sub: parts.filter((p) => p !== dur).join(' · ') || text(cols[2]), dur, img: th?.thumbnail?.thumbnails?.at(-1)?.url || '',
        round: /CIRCLE/.test(th?.thumbnailCrop || ''), open: !(ep.watchEndpoint || ep.watchPlaylistEndpoint), ep });
    };
    walk(j.contents);
    return out;
  };
  const ytmPlay = (it) => document.querySelector('ytmusic-app')?.dispatchEvent(new CustomEvent('yt-navigate', { bubbles: true, composed: true, detail: { endpoint: it.ep } }));
  const SP_PATH = { songs: '/tracks', albums: '/albums', artists: '/artists', playlists: '/playlists' };
  const spGo = (path) => { history.pushState({}, '', path); dispatchEvent(new PopStateEvent('popstate', { state: {} })); };
  // resultados: filas (Todo), filas de canciones (Canciones) o tarjetas con su botón de play (Álbumes, Artistas, Listas)
  const spNodes = () => {
    const rows = [...document.querySelectorAll('#main-view [data-encore-id="listRow"], #main-view [data-testid="tracklist-row"]')];
    if (rows.length) return rows;
    return [...document.querySelectorAll('#main-view [data-encore-id="cardTitle"]')].map((t) => {
      let c = t; while (c && !c.querySelector('[data-testid="play-button"]')) c = c.parentElement;
      return c;
    }).filter(Boolean);
  };
  const spHref = (n) => (n.querySelector('[data-testid="title"] a, a[href*="/track/"], [data-encore-id="cardTitle"] a') || n.querySelector('a[href]') || n.closest('a[href]'))?.getAttribute('href') || '';
  const spItem = (n, kind) => {
    const href = spHref(n), img = n.querySelector('[data-testid="entity-image"], [data-testid="card-image"] img, img[data-testid="card-image"], img');
    const base = { href, img: img?.currentSrc || img?.getAttribute('src') || '', round: /\/artist\//.test(href), open: false, dur: '' };
    if (n.matches('[data-testid="tracklist-row"]')) {
      return { ...base, title: n.querySelector('a[href*="/track/"]')?.textContent || '', kind: 'Canción',
        sub: [...n.querySelectorAll('a[href*="/artist/"]')].map((a) => a.textContent).join(', '), dur: (n.innerText.match(/\d+:\d\d/g) || []).pop() || '' };
    }
    const ct = n.querySelector('[data-encore-id="cardTitle"]');
    if (ct) return { ...base, title: ct.textContent, kind: KIND_NAME[kind] || '', sub: n.querySelector('[data-encore-id="cardSubtitle"]')?.textContent || '' };
    const sub = [...(n.querySelector('[data-testid="subtitle"] span')?.children || [])].map((x) => x.textContent.trim()).filter(Boolean);
    const k = KINDS.test(sub[0] || '') ? sub.shift() : KIND_NAME[kind] || '';
    return { ...base, title: n.querySelector('[data-testid="title"] a')?.textContent || '', kind: k, sub: sub.join(', ') };
  };
  const spSearch = async (q, kind) => {
    const path = '/search/' + encodeURIComponent(q) + (SP_PATH[kind] || '');
    const here = location.pathname.replace(/^\/intl-[^/]+/, '') === path;
    const old = spNodes()[0];
    if (!here) spGo(path);
    let nodes = spNodes();
    for (let i = 0; i < 50 && !(nodes.length && (here || nodes[0] !== old)); i++) { await wait(120); nodes = spNodes(); }
    return nodes.slice(0, 12).map((n) => spItem(n, kind));
  };
  const spPlay = async (it) => {
    const n = spNodes().find((x) => spHref(x) === it.href);
    if (n && !it.round) {                                       // canción o lista: doble clic en la fila o el play de la tarjeta, como en Spotify
      const b = n.querySelector('[data-testid="play-button"]');
      return b ? b.click() : n.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    }
    spGo(it.href.replace(/^\/intl-[^/]+/, ''));                // artista (o la lista cambió): abrir su página y tocar su play
    for (let i = 0; i < 40; i++) {
      await wait(150);
      const b = document.querySelector('#main-view [data-testid="action-bar-row"] [data-testid="play-button"]');
      if (b) return b.click();
    }
  };
  window.__lm.search = async (q, kind, seq) => {
    let items = [], error = '';
    try { found = await (SP ? spSearch : ytmSearch)(q, kind); items = found.map(({ ep, ...x }) => x); } catch (e) { error = String(e); }
    invoke && invoke('search_results', { seq, items, error }).catch(() => {});
  };
  window.__lm.searchPlay = (i) => { const it = found[i]; if (it) (SP ? spPlay : ytmPlay)(it); };
  tick();
})();
