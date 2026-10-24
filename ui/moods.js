/* Moods: personas haciendo una actividad (pixel art 48x48).
   Cada mood tiene una pose base y 2 acciones; cada 15-30 s se hace una acción y se vuelve a la base.
   Usa los helpers de float.html (px, ell, blob, tri, DARK). Se redibuja a ~12 fps solo mientras suena. */
const SKIN = '#f1c9a0', SKIN_S = '#d9a77c';
const clamp01 = (x) => Math.max(0, Math.min(1, x));
const lerp = (a, b, k) => a + (b - a) * k;
const smooth = (x) => { x = clamp01(x); return x * x * (3 - 2 * x); };
// 0 -> 1 (se mantiene) -> 0: sube rápido, mantiene, baja rápido
const hold = (u) => smooth(Math.min(u / 0.2, (1 - u) / 0.2));
const bell = (u) => Math.sin(Math.PI * clamp01(u));

function limb(c, x1, y1, x2, y2, col, r = 3.2) {
  const n = Math.max(1, Math.ceil(Math.hypot(x2 - x1, y2 - y1)));
  [[r + 1, DARK], [r, col]].forEach(([rad, color]) => {
    for (let i = 0; i <= n; i++) ell(c, x1 + (x2 - x1) * i / n, y1 + (y2 - y1) * i / n, rad, rad, color);
  });
}
const hand = (c, x, y) => { ell(c, x, y, 3, 3, DARK); ell(c, x, y, 2.1, 2.1, SKIN); };
const mugPx = (c, x, y) => {
  px(c, x - 1, y - 1, 7, 7, DARK); px(c, x, y, 5, 5, '#fff'); px(c, x + 4, y, 1, 5, '#ddd');
  px(c, x + 6, y + 1, 2, 3, DARK); px(c, x + 6, y + 2, 1, 1, '#fff'); px(c, x + 1, y, 3, 1, '#7a4a2a');
};
const steam = (c, x, y, t) => {
  for (let i = 0; i < 3; i++) {
    const age = ((t * 0.7 + i / 3) % 1);
    c.globalAlpha = 1 - age; px(c, x + Math.round(Math.sin(age * 6 + i) * 1.2), y - Math.round(age * 7), 1, 2, '#fff'); c.globalAlpha = 1;
  }
};

/* ---------- persona ---------- */
function personBody(c, o) {
  const bd = o.bodyDy || 0, bx = o.bodyDx || 0;
  if (o.hood) blob(c, 24 + (o.dx || 0), 25 + (o.dy || 0), 11, 12, o.hood, o.hoodS);
  blob(c, 24 + bx, 43 + bd, 12, 10, o.shirt, o.shirtS);
  if (o.blanket) { blob(c, 24, 48, 19, 9, '#a98bf0', '#7a5fc4'); px(c, 8, 41, 32, 1, '#c8b3ff'); }
}
function personArms(c, o) {
  const bd = o.bodyDy || 0, bx = o.bodyDx || 0;
  limb(c, 14 + bx, 40 + bd, o.L[0], o.L[1], o.shirt); limb(c, 34 + bx, 40 + bd, o.R[0], o.R[1], o.shirt);
}
function personHead(c, o) {
  const dx = Math.round(o.dx || 0), dy = Math.round(o.dy || 0), e = o.eyes || 'open', ex = o.eyeDx || 0;
  c.save(); c.translate(dx, dy);
  px(c, 22, 30, 4, 3, SKIN_S);
  blob(c, 24, 22, 8, 9, SKIN, SKIN_S);
  ell(c, 24, 15, 8.6, 5.2, o.hair); px(c, 15, 16, 2, 5, o.hair); px(c, 31, 16, 2, 5, o.hair); px(c, 17, 17, 14, 1, o.hair);
  if (e === 'happy') { [19, 26].forEach(x => { px(c, x, 22, 1, 1, DARK); px(c, x + 1, 21, 2, 1, DARK); px(c, x + 3, 22, 1, 1, DARK); }); }
  else if (e === 'closed') { px(c, 19, 22, 4, 1, DARK); px(c, 26, 22, 4, 1, DARK); }
  else if (e === 'sad') { px(c, 19, 22, 4, 1, DARK); px(c, 26, 22, 4, 1, DARK); px(c, 18, 20, 3, 1, DARK); px(c, 28, 20, 3, 1, DARK); }
  else { [20, 27].forEach(x => { px(c, x + ex, 21, 2, 3, DARK); px(c, x + ex, 21, 1, 1, '#fff'); }); }
  const m = o.mouth || 'smile';
  if (m === 'open') { px(c, 22, 26, 5, 3, DARK); px(c, 23, 28, 3, 1, '#ff8fa3'); }
  else if (m === 'yawn') { px(c, 22, 25, 5, 5, DARK); px(c, 23, 28, 3, 2, '#ff8fa3'); }
  else if (m === 'sad') { px(c, 22, 28, 5, 1, '#b5555a'); px(c, 21, 29, 1, 1, '#b5555a'); px(c, 27, 29, 1, 1, '#b5555a'); }
  else if (m === 'sip') { px(c, 23, 27, 3, 1, '#b5555a'); }
  else { px(c, 22, 27, 5, 1, '#b5555a'); px(c, 21, 26, 1, 1, '#b5555a'); px(c, 27, 26, 1, 1, '#b5555a'); }
  px(c, 18, 25, 2, 1, '#f2907f'); px(c, 29, 25, 2, 1, '#f2907f');
  if (o.glasses) {
    [[18, 20], [26, 20]].forEach(([x, y]) => { px(c, x, y, 5, 5, '#1c1c28'); px(c, x + 1, y + 1, 3, 3, o.glow || '#7fe9ff'); });
    px(c, 23, 22, 3, 1, '#1c1c28');
  }
  if (o.phones) {
    for (let a = Math.PI; a <= 2 * Math.PI; a += .02) px(c, Math.round(24 + 11 * Math.cos(a)), Math.round(21 + 12 * Math.sin(a)), 2, 2, '#3d3d52');
    [12, 36].forEach(x => blob(c, x, 23, 2.5, 4.5, '#ff3b66', '#b81c43'));
  }
  if (o.hat) { tri(c, [18, 13, 24, 0, 30, 13], '#ffd966'); px(c, 21, 6, 2, 1, '#ff4d6d'); px(c, 23, 9, 3, 1, '#ff4d6d'); ell(c, 24, 1, 1.7, 1.7, '#fff'); }
  c.restore();
}
const deskTop = (c, col, hi) => { px(c, 0, 44, 48, 4, DARK); px(c, 0, 45, 48, 3, col); px(c, 0, 45, 48, 1, hi); };
const laptop = (c, glow) => {
  px(c, 11, 32, 26, 11, DARK); px(c, 12, 33, 24, 9, '#c9ced9'); px(c, 12, 33, 23, 8, '#d7dbe4'); px(c, 22, 36, 4, 3, glow || '#9aa1b3');
  px(c, 9, 42, 30, 3, DARK); px(c, 10, 42, 28, 2, '#aab0be');
};

/* ---------- escenas ---------- */
const SCENES = {
  trabajando(c, t, a, paused) {
    const k = Math.floor(t * 6) % 2, h = a ? hold(a.u) : 0;
    const typing = [[19 + k, 43 - k], [29 - k, 42 + k]];
    const o = { shirt: '#4a7bd0', shirtS: '#365ba3', hair: '#4a3020', hood: '#4a7bd0', hoodS: '#365ba3', glasses: 1, glow: '#cfe9ff', phones: 1, eyes: paused ? 'closed' : (t % 4.6 < .12 ? 'closed' : 'open'), dy: paused ? 1 : 0 };
    o.L = typing[0]; o.R = typing[1];
    let mugX = 3, mugY = 38;
    if (a && a.name === 'cafe') {
      const reach = smooth(a.u / 0.2), sip = hold(a.u);
      o.L = [lerp(typing[0][0], 23, sip), lerp(typing[0][1], 31, sip)];
      mugX = lerp(3, o.L[0] - 5, reach); mugY = lerp(38, o.L[1] - 3, reach);
      o.dy = -Math.round(h); o.mouth = h > .6 ? 'sip' : 'smile'; o.eyes = h > .6 ? 'closed' : o.eyes;
    }
    if (a && a.name === 'estirar') {
      o.L = [lerp(typing[0][0], 6, h), lerp(typing[0][1], 15, h)]; o.R = [lerp(typing[1][0], 42, h), lerp(typing[1][1], 15, h)];
      o.dy = -Math.round(h); o.bodyDy = -Math.round(h); o.mouth = h > .5 ? 'yawn' : 'smile'; o.eyes = h > .5 ? 'closed' : o.eyes;
    }
    deskTop(c, '#9a6a40', '#b98558');
    personBody(c, o); personArms(c, o); personHead(c, o);
    laptop(c, '#9aa1b3');
    if (!(a && a.name === 'estirar' && h > .3)) { hand(c, o.L[0], o.L[1]); hand(c, o.R[0], o.R[1]); }
    else { hand(c, o.L[0], o.L[1]); hand(c, o.R[0], o.R[1]); }
    mugPx(c, Math.round(mugX), Math.round(mugY));
    if (!(a && a.name === 'cafe' && h > .3)) steam(c, Math.round(mugX) + 2, Math.round(mugY) - 1, t);
    px(c, 38, 41, 8, 3, '#fff'); px(c, 38, 40, 7, 1, '#ddd'); px(c, 39, 38, 6, 2, '#ffd966');
  },

  programando(c, t, a, paused) {
    const k = Math.floor(t * 10) % 2, h = a ? hold(a.u) : 0;
    const typing = [[19 + k, 43 - k], [29 - k, 43 - (1 - k)]];
    const o = { shirt: '#3a3f55', shirtS: '#262a3c', hair: '#222', hood: '#3a3f55', hoodS: '#262a3c', glasses: 1, phones: 1,
                eyes: paused ? 'closed' : 'open', dy: paused ? 1 : 0, dx: paused ? 0 : (k ? 0 : 1) * 0 };
    o.L = typing[0]; o.R = typing[1];
    let canX = 40, canY = 34;
    if (a && a.name === 'bebida') {
      const reach = smooth(a.u / 0.2);
      o.R = [lerp(typing[1][0], 27, h), lerp(typing[1][1], 31, h)];
      canX = lerp(40, o.R[0] - 2, reach); canY = lerp(34, o.R[1] - 4, reach);
      o.dy = -Math.round(h); o.mouth = h > .6 ? 'sip' : 'smile';
    }
    if (a && a.name === 'pensar') {
      o.R = [lerp(typing[1][0], 27, h), lerp(typing[1][1], 30, h)];
      o.dx = Math.round(h); o.mouth = 'sip';
    }
    // código flotando
    ['#5ef28a', '#7fe9ff', '#ffd966', '#5ef28a'].forEach((col, i) => {
      const w = 4 + ((i * 5) % 7), off = ((t * 3 + i * 4) % 14);
      px(c, 2 + Math.round(off % 3), 4 + i * 5, w, 1, col);
    });
    deskTop(c, '#2a2f45', '#3a4160');
    personBody(c, o); personArms(c, o); personHead(c, o);
    px(c, 11, 32, 26, 11, DARK); px(c, 12, 33, 24, 9, '#20232f'); px(c, 22, 36, 4, 3, (Math.floor(t * 4) % 2) ? '#7fe9ff' : '#4cb7d4');
    px(c, 9, 42, 30, 3, DARK); px(c, 10, 42, 28, 2, '#161824');
    hand(c, o.L[0], o.L[1]); hand(c, o.R[0], o.R[1]);
    rr2(c, Math.round(canX), Math.round(canY), 5, 9, '#3ddc84'); px(c, Math.round(canX) + 1, Math.round(canY) + 2, 3, 2, '#fff');
    if (a && a.name === 'pensar' && h > .4 && Math.floor(t * 4) % 2 === 0) {            // idea
      ell(c, 38, 7, 3.5, 3.5, DARK); ell(c, 38, 7, 2.5, 2.5, '#ffd966'); px(c, 37, 11, 3, 2, '#9aa1b3');
      px(c, 33, 3, 1, 1, '#ffd966'); px(c, 43, 3, 1, 1, '#ffd966'); px(c, 44, 8, 1, 1, '#ffd966');
    }
  },

  tradeando(c, t, a, paused) {
    const h = a ? hold(a.u) : 0, up = a && a.name === 'sube', down = a && a.name === 'baja';
    // velas: caminata pseudoaleatoria determinista que avanza cada 0.9 s
    const step = Math.floor(t / 0.9), N = 6, candles = [];
    let price = 10;
    for (let i = step - 12; i <= step; i++) {
      const r = Math.sin(i * 12.9898) * 43758.5453; const rnd = r - Math.floor(r);
      const o0 = price; price = Math.max(3, Math.min(17, price + (rnd - .5) * 6));
      if (i > step - N) candles.push({ o: o0, c: price });
    }
    if (up) candles[N - 1] = { o: 8, c: 16 - (1 - h) * 6 }; if (down) candles[N - 1] = { o: 12, c: 12 - 8 * h };
    const o = { shirt: '#2f6f5e', shirtS: '#1f4f43', hair: '#6b3b1b', hood: '#2f6f5e', hoodS: '#1f4f43', glasses: 1, glow: '#9ff0b8', eyes: paused ? 'closed' : (t % 5 < .12 ? 'closed' : 'open'), eyeDx: 1,
                L: [18, 43], R: [28 + (Math.floor(t * 2) % 2), 43] };
    if (up) { o.L = [lerp(18, 10, h), lerp(43, 17, h)]; o.R = [lerp(28, 35, h), lerp(43, 17, h)]; o.eyes = 'happy'; o.mouth = 'open'; o.dy = -Math.round(h); }
    if (down) { o.R = [lerp(28, 25, h), lerp(43, 12, h)]; o.eyes = h > .5 ? 'sad' : o.eyes; o.mouth = h > .5 ? 'sad' : 'smile'; o.dy = Math.round(h); }
    deskTop(c, '#2b2b3d', '#3d3d55');
    // monitor
    px(c, 24, 4, 24, 26, DARK); px(c, 25, 5, 22, 24, '#10131c');
    candles.forEach((cd, i) => {
      const col = cd.c >= cd.o ? '#3ddc84' : '#ff4d6d', x = 27 + i * 3.4 | 0;
      const hi = 26 - Math.max(cd.o, cd.c) - 1, lo = 26 - Math.min(cd.o, cd.c) + 1;
      px(c, x + 1, hi, 1, lo - hi + 1, col); px(c, x, 26 - Math.max(cd.o, cd.c), 3, Math.max(1, Math.abs(cd.c - cd.o)), col);
    });
    px(c, 34, 30, 5, 5, DARK); px(c, 30, 34, 13, 2, '#444a5e'); px(c, 35, 30, 3, 4, '#444a5e');
    c.save(); c.translate(-9, 0);
    personBody(c, o); personArms(c, o); personHead(c, o);
    px(c, 12, 41, 26, 3, DARK); px(c, 13, 41, 24, 2, '#444a5e');
    if (!up) hand(c, o.L[0], o.L[1]); else hand(c, o.L[0], o.L[1]);
    hand(c, o.R[0], o.R[1]);
    c.restore();
    if (up && h > .3) { ell(c, 43, 3, 2.7, 2.7, '#c99a1c'); ell(c, 43, 3, 1.8, 1.8, '#ffd966'); if (Math.floor(t * 6) % 2) px(c, 40, 0, 1, 1, '#fff'); }
    if (down && h > .5) { px(c, 20, 6 + Math.floor(t * 4) % 3, 2, 3, '#7fe9ff'); }
  },

  descansando(c, t, a, paused) {
    const h = a ? hold(a.u) : 0, beat = Math.floor(t * 1.6) % 2, br = Math.floor(t * 0.9) % 2;
    ell(c, 24, 25, 15, 15, '#cdb8ff');
    const o = { shirt: '#6b7bd6', shirtS: '#4a57a8', hair: '#3a2418', hood: '#6b7bd6', hoodS: '#4a57a8', glasses: 1, glow: '#ffe9a8', phones: 1, eyes: 'closed', blanket: 1, bodyDy: paused ? 1 : br,
                dy: paused ? 1 : (beat && !a ? 1 : 0), mouth: 'smile', L: [30, 41], R: [20, 41] };
    let mug = [31, 36];
    if (a && a.name === 'sorbo') { o.R = [lerp(30, 24, h), lerp(41, 30, h)]; mug = [lerp(31, o.R[0] - 2, smooth(a.u / .2)), lerp(36, o.R[1] - 4, smooth(a.u / .2))]; o.dy = -Math.round(h); o.mouth = h > .6 ? 'sip' : 'smile'; }
    if (a && a.name === 'bostezo') { o.L = [lerp(30, 6, h), lerp(41, 16, h)]; o.R = [lerp(20, 42, h), lerp(41, 16, h)]; o.mouth = h > .4 ? 'yawn' : 'smile'; o.dy = -Math.round(h); }
    personBody(c, o); personArms(c, o); personHead(c, o);
    blob(c, 24, 47, 20, 8, '#a98bf0', '#7a5fc4'); px(c, 10, 41, 28, 1, '#c8b3ff');
    hand(c, o.L[0], o.L[1]); hand(c, o.R[0], o.R[1]);
    if (!(a && a.name === 'bostezo' && h > .3)) { mugPx(c, Math.round(mug[0]), Math.round(mug[1])); steam(c, Math.round(mug[0]) + 2, Math.round(mug[1]) - 1, t); }
    px(c, 3, 38, 2, 9, '#3b8f4a'); tri(c, [1, 36, 3, 29, 6, 36], '#4fc062'); tri(c, [3, 38, 7, 31, 9, 38], '#3b8f4a');
    if (paused || (a && a.name === 'bostezo' && h > .5)) { px(c, 40, 8, 3, 1, '#fff'); px(c, 41, 9, 1, 1, '#fff'); px(c, 40, 10, 3, 1, '#fff'); }
  },

  festejando(c, t, a, paused) {
    const k = Math.floor(t * 2.5) % 2, h = a ? hold(a.u) : 0, cols = ['#ff4d6d', '#ffd966', '#5ef28a', '#7fe9ff', '#c08cff'];
    const jump = a && a.name === 'salto' ? Math.round(4 * Math.abs(Math.sin(a.u * Math.PI * 2.5)) * (a.u < .85 ? 1 : 0)) : 0;
    const sway = paused ? 0 : (k ? 1 : -1);
    const o = { shirt: '#ff6fa5', shirtS: '#c8487d', hair: '#2b1b3a', glasses: 1, glow: '#ffb3d1', eyes: 'happy', mouth: 'open', hat: 1, dx: sway, dy: -jump, bodyDy: -jump, bodyDx: sway,
                L: k ? [6, 14] : [9, 42], R: k ? [39, 42] : [42, 14] };
    if (paused) { o.L = [10, 44]; o.R = [38, 44]; o.mouth = 'smile'; o.eyes = 'closed'; }
    if (a && (a.name === 'confeti' || a.name === 'salto')) { o.L = [lerp(o.L[0], 7, h), lerp(o.L[1], 12 - jump, h)]; o.R = [lerp(o.R[0], 41, h), lerp(o.R[1], 12 - jump, h)]; }
    // confeti cayendo
    if (!paused) for (let i = 0; i < 22; i++) {
      const sp = 5 + (i % 4) * 2, y = ((i * 17 + t * sp) % 52) - 3, x = ((i * 29) % 46) + 1 + Math.round(Math.sin(t * 1.5 + i) * 1.5);
      px(c, x, Math.round(y), 1, 2, cols[i % 5]);
    }
    personBody(c, o); personArms(c, o); personHead(c, o); hand(c, o.L[0], o.L[1]); hand(c, o.R[0], o.R[1]);
    if (a && a.name === 'confeti') {                                                        // explosión
      for (let i = 0; i < 26; i++) {
        const ang = i * 2.4, r = a.u * (10 + (i % 5) * 4);
        px(c, Math.round(24 + Math.cos(ang) * r), Math.round(10 + Math.sin(ang) * r * .8 + a.u * a.u * 14), 2, 2, cols[i % 5]);
      }
    }
  },
};
function rr2(c, x, y, w, h, col) { px(c, x - 1, y, w + 2, h, DARK); px(c, x, y - 1, w, h + 2, DARK); px(c, x, y, w, h, col); }

const MOODS = {
  trabajando: { actions: [['cafe', 4.2], ['estirar', 3.6]] },
  programando: { actions: [['bebida', 4.2], ['pensar', 4.5]] },
  tradeando: { actions: [['sube', 4.5], ['baja', 4.5]] },
  descansando: { actions: [['sorbo', 4.5], ['bostezo', 4]] },
  festejando: { actions: [['confeti', 3.5], ['salto', 3.2]] },
};
// Estado de una escena: pose base y, cada 15-30 s, una acción al azar que dura unos segundos.
function moodTick(st, now) {
  if (st.action && now >= st.action.start + st.action.dur) { st.action = null; st.next = now + 15 + Math.random() * 15; }
  if (!st.action && now >= st.next) {
    const [name, dur] = MOODS[st.mood].actions[Math.floor(Math.random() * 2)];
    st.action = { name, dur, start: now, u: 0 };
  }
  if (st.action) st.action.u = (now - st.action.start) / st.action.dur;
}
window.MOODS = MOODS; window.SCENES = SCENES; window.moodTick = moodTick;
