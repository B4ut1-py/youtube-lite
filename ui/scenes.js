/* Habitaciones "lo-fi" de cada aspecto: fondo pixel art de 96x96 que se dibuja una sola vez al elegir el aspecto.
   El personaje (perro, gato o mood) se dibuja aparte, encima, a 64 px. Usa ell/tri de float.html con S = 1. */
const hex = (h) => [1, 3, 5].map((i) => parseInt(h.substr(i, 2), 16));
const mix = (a, b, k) => '#' + hex(a).map((v, i) => Math.round(v * (1 - k) + hex(b)[i] * k).toString(16).padStart(2, '0')).join('');
const BAYER = [0, .5, .75, .25];
// degradé vertical con tramado ordenado (el "pixel art" de los degradés)
function dgrad(c, x0, y0, w, y1, c1, c2, steps = 6) {
  for (let y = y0; y < y1; y++) for (let x = x0; x < x0 + w; x++) {
    const t = (y - y0) / (y1 - y0) * steps, l = Math.floor(t), f = t - l;
    c.fillStyle = mix(c1, c2, Math.min(steps, l + (f > BAYER[(x % 2) + 2 * (y % 2)] ? 1 : 0)) / steps); c.fillRect(x, y, 1, 1);
  }
}
const P = (c, x, y, w, h, col) => { c.fillStyle = col; c.fillRect(x, y, w, h); };
const dots = (c, pts, col) => pts.forEach(([x, y]) => P(c, x, y, 1, 1, col));
// halo detrás del personaje: dos círculos lisos de luz (sin tramado, que en pixel art se ve como ruido)
function halo(c, col) { c.globalAlpha = .07; ell(c, 48, 60, 34, 34, col); c.globalAlpha = .08; ell(c, 48, 60, 23, 23, col); c.globalAlpha = 1; }
function shadow(c, rx) { c.globalAlpha = .35; ell(c, 48, 94, rx, 2.4, '#0c0716'); c.globalAlpha = 1; }
function room(c, wall1, wall2, floor1, floor2) { dgrad(c, 0, 0, 96, 72, wall1, wall2); P(c, 0, 72, 96, 2, mix(wall2, '#000000', .35)); dgrad(c, 0, 74, 96, 96, floor1, floor2, 3); }
// escritorio a lo ancho, a la misma altura que el de los moods (fila 44 de su rejilla de 48)
function desk(c, col, hi) { P(c, 0, 91, 96, 1, '#2a2233'); P(c, 0, 92, 96, 4, col); P(c, 0, 92, 96, 1, hi); }
function windowNight(c, x, y, w, h) {
  P(c, x - 2, y - 2, w + 4, h + 4, '#1b1c3a'); dgrad(c, x, y, w, y + h, '#0f1335', '#2b2f6e', 4);
  ell(c, x + w - 8, y + 7, 4, 4, '#fff1c2'); ell(c, x + w - 6.5, y + 6, 3.4, 3.4, '#0f1335');
  dots(c, [[x + 4, y + 4], [x + 9, y + 11], [x + 5, y + h - 6], [x + 14, y + 5], [x + w - 4, y + h - 9]], '#d9e1ff');
  P(c, x + (w >> 1), y, 1, h, '#1b1c3a'); P(c, x, y + (h >> 1), w, 1, '#1b1c3a'); P(c, x - 3, y + h + 2, w + 6, 2, '#53486e');
}
function windowDay(c, x, y, w, h) {
  P(c, x - 2, y - 2, w + 4, h + 4, '#ffffff'); dgrad(c, x, y, w, y + h, '#8fd0ff', '#d6f0ff', 4);
  ell(c, x + 8, y + 9, 5, 2.4, '#ffffff'); ell(c, x + 12, y + 8, 4, 2.6, '#ffffff'); ell(c, x + w - 7, y + h - 6, 4, 2, '#ffffff');
  P(c, x + (w >> 1), y, 1, h, '#ffffff'); P(c, x - 3, y + h + 2, w + 6, 2, '#e3ecf7');
}
function plant(c, x, y) { P(c, x, y, 8, 7, '#8c4a3c'); P(c, x + 1, y + 1, 6, 1, '#b4614c'); tri(c, [x - 2, y, x + 3, y - 12, x + 5, y], '#3f9a5d'); tri(c, [x + 3, y, x + 8, y - 9, x + 10, y], '#57b971'); tri(c, [x + 1, y, x + 4, y - 7, x + 7, y], '#2f7a49'); }
function shelf(c, x, y, w, cols) { P(c, x, y, w, 2, '#5b3f2f'); let bx = x + 1; cols.forEach((col, i) => { const h = 7 + (i * 3) % 4; P(c, bx, y - h, 3, h, col); P(c, bx, y - h, 3, 1, mix(col, '#ffffff', .3)); bx += 4; }); }
const BOOKS = ['#ff6b8b', '#ffd166', '#6bd3ff', '#9b7bff', '#57d38c', '#ff9f5a'];

const ROOMS = {
  dog(c) {                                   // cuarto de noche: ventana con luna, estantes, póster, lámpara y alfombra
    room(c, '#2d3a86', '#4a5eb6', '#3c2b64', '#2b1f4a');
    windowNight(c, 8, 10, 30, 26); shelf(c, 60, 30, 26, BOOKS);
    P(c, 66, 46, 18, 14, '#1d1f45'); P(c, 67, 47, 16, 12, '#ffb84d'); P(c, 67, 47, 16, 3, '#ff7a59'); ell(c, 75, 54, 3, 3, '#fff3c4');
    P(c, 10, 58, 9, 2, '#e7c48a'); P(c, 13, 60, 3, 14, '#54436e'); P(c, 9, 52, 11, 6, '#ffd59a');
    ell(c, 48, 88, 30, 6, '#5a3f86'); ell(c, 48, 88, 26, 4.5, '#6c50a0');
    halo(c, '#ffe6b0'); shadow(c, 22);
  },
  cat(c) {                                   // atardecer: persiana, estante, plantas y almohadón
    room(c, '#ff9f86', '#d9607f', '#8a3f63', '#5e2a4c');
    P(c, 54, 8, 32, 30, '#b14d69'); dgrad(c, 56, 10, 28, 36, '#ffd59a', '#ff8a6b', 4); ell(c, 70, 30, 7, 7, '#fff1c2');
    for (let y = 12; y < 26; y += 3) P(c, 56, y, 28, 1, '#e98a6c'); P(c, 52, 38, 36, 2, '#ffd7c4');
    shelf(c, 6, 34, 24, ['#3f9a5d', '#ffd166', '#6b7bff', '#ffffff', '#ff6b8b']); plant(c, 6, 66); plant(c, 84, 66);
    ell(c, 48, 90, 24, 6, '#3b6fd8'); ell(c, 48, 89, 22, 4.5, '#5a8ef0');
    halo(c, '#fff3d0'); shadow(c, 21);
  },
  trabajando(c) {                            // oficina de día
    room(c, '#cfe4ff', '#93b8f0', '#9a7a5c', '#7a5c44');
    windowDay(c, 60, 8, 28, 24); shelf(c, 6, 28, 24, BOOKS); plant(c, 84, 82);
    P(c, 8, 40, 14, 10, '#ffffff'); P(c, 9, 41, 12, 8, '#ffe08a'); P(c, 10, 43, 7, 1, '#e0a040'); P(c, 10, 46, 9, 1, '#e0a040');   // nota en la pared
    halo(c, '#ffffff'); desk(c, '#9a6a40', '#b98558');
  },
  programando(c) {                           // cuarto oscuro con luz violeta y código en la pared
    room(c, '#141833', '#262c56', '#1b1d33', '#111226');
    P(c, 0, 4, 96, 2, '#7a5cff'); c.globalAlpha = .08; P(c, 0, 6, 96, 10, '#7a5cff'); c.globalAlpha = 1;
    shelf(c, 6, 30, 22, ['#57d38c', '#6bd3ff', '#ff6b8b', '#ffd166', '#9b7bff']); shelf(c, 68, 20, 22, ['#6bd3ff', '#9b7bff', '#57d38c', '#ff6b8b', '#ffd166']);
    P(c, 70, 30, 22, 16, '#0a0c1a'); P(c, 71, 31, 20, 14, '#11162e');
    [[73, 34, 9, '#57d38c'], [73, 37, 13, '#6bd3ff'], [75, 40, 8, '#ffd166'], [73, 43, 11, '#ff6b8b']].forEach(([x, y, w, col]) => P(c, x, y, w, 1, col));
    plant(c, 4, 66);
    halo(c, '#6bd3ff'); desk(c, '#2a2f45', '#3a4160');
  },
  tradeando(c) {                             // sala de trading: cinta de cotizaciones y luces verdes y rojas
    room(c, '#1a1d30', '#2c3250', '#202336', '#151726');
    P(c, 0, 6, 96, 7, '#0b0d18'); [[3, '#3ddc84'], [17, '#ff4d6d'], [31, '#3ddc84'], [45, '#3ddc84'], [59, '#ff4d6d'], [73, '#3ddc84'], [87, '#ffd166']].forEach(([x, col]) => { P(c, x, 8, 7, 1, col); P(c, x, 10, 4, 1, mix(col, '#0b0d18', .4)); });
    P(c, 4, 22, 20, 15, '#0a0c16'); P(c, 5, 23, 18, 13, '#10142a');
    for (let i = 0; i < 6; i++) { const h = [5, 8, 6, 10, 7, 11][i]; P(c, 7 + i * 3, 34 - h, 2, h, i % 3 === 1 ? '#ff4d6d' : '#3ddc84'); }
    plant(c, 84, 66);
    halo(c, '#3ddc84'); desk(c, '#2b2b3d', '#3d3d55');
  },
  descansando(c) {                           // cuarto lila de noche con lucecitas
    room(c, '#b9a6f2', '#8a74d8', '#6a54a8', '#4f3d86');
    windowNight(c, 8, 10, 26, 22);
    for (let i = 0; i < 12; i++) { const x = 40 + i * 4.6, y = 8 + Math.round(Math.sin(i * .9) * 2 + i * .2); P(c, x | 0, y, 1, 1, '#5b4a8a'); P(c, x | 0, y + 1, 2, 2, ['#ffd166', '#ff8fb1', '#8fe3ff'][i % 3]); }
    shelf(c, 66, 36, 22, ['#ffffff', '#ffd166', '#ff8fb1', '#8fe3ff', '#57d38c']); plant(c, 84, 66);
    halo(c, '#ffffff');
  },
  estudiando(c) {                            // noche de estudio: ventana, estantes y lámpara de escritorio
    room(c, '#3a2c55', '#5b4478', '#4a3348', '#33223a');
    windowNight(c, 60, 10, 28, 24); shelf(c, 6, 26, 26, BOOKS); shelf(c, 6, 44, 26, ['#9b7bff', '#57d38c', '#ffd166', '#ff6b8b', '#6bd3ff', '#ffffff']);
    halo(c, '#ffd59a'); desk(c, '#7a5a3a', '#9a7650');
    c.globalAlpha = .2; c.fillStyle = '#ffd98a'; c.beginPath(); c.moveTo(19, 70); c.lineTo(27, 70); c.lineTo(40, 91); c.lineTo(10, 91); c.fill(); c.globalAlpha = 1;
    ell(c, 8, 89, 6, 1.6, '#1d1830'); P(c, 3, 88, 11, 2, '#2c2f45'); P(c, 4, 88, 9, 1, '#4a4f72');
    for (let i = 0; i <= 12; i++) P(c, 7 + Math.round(i * .25), 87 - i, 2, 1, '#2c2f45');
    for (let i = 0; i <= 9; i++) P(c, 10 + i, 75 - Math.round(i * .9), 2, 1, '#2c2f45');
    ell(c, 10.5, 75, 1.6, 1.6, '#4a4f72');
    P(c, 20, 63, 6, 2, '#1d1830'); tri(c, [18, 64, 28, 64, 31, 70], '#1d1830'); tri(c, [18, 64, 15, 70, 31, 70], '#1d1830');
    tri(c, [19, 65, 27, 65, 29, 69], '#e2575c'); tri(c, [19, 65, 17, 69, 29, 69], '#e2575c'); P(c, 20, 65, 4, 1, '#ff8a8a');
    P(c, 21, 70, 4, 1, '#fff2b8');
  },
  festejando(c) {                            // fiesta: guirnalda de banderines y globos
    room(c, '#ff9cc6', '#8a64ff', '#6a3fd0', '#4e2ca8');
    for (let i = 0; i < 9; i++) { const x = 2 + i * 11, y = 6 + Math.round(Math.sin(i * .7) * 1.5); tri(c, [x, y, x + 8, y, x + 4, y + 7], ['#ffd166', '#57d38c', '#6bd3ff', '#ff6b8b'][i % 4]); }
    P(c, 0, 6, 96, 1, '#ffffffaa');
    [[10, 30, '#ffd166'], [84, 26, '#6bd3ff'], [88, 40, '#ff6b8b']].forEach(([x, y, col]) => { ell(c, x, y, 5, 6, col); P(c, x - 1, y - 4, 2, 2, '#ffffff99'); P(c, x, y + 6, 1, 12, '#ffffff88'); });
    halo(c, '#ffffff'); shadow(c, 22);
  },
};
window.ROOMS = ROOMS;
