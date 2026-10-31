// Tema: 'dark' | 'light' | 'auto' (sigue el modo de Windows). Pone o quita html.light.
// Con el Cristal prendido manda el tono de su fondo (claro u oscuro) en lugar del tema.
(() => {
let theme = 'auto', glassTone = null;
const mq = matchMedia('(prefers-color-scheme: dark)');
const setLight = () => document.documentElement.classList.toggle('light', glassTone ? glassTone === 'light' : theme === 'light' || (theme !== 'dark' && !mq.matches));
mq.onchange = setLight;
window.applyTheme = (t) => { theme = t; setLight(); };
// tono forzado ('dark' | 'light') o null para volver al tema; lo usa el flotante cuando toma el fondo de la app
window.applyTone = (t) => { glassTone = t; setLight(); };
// Cristal: g = null (apagado) o { tone, tint } (ver glass_spec en main.rs). Pone html.glass y el fondo fijo (#lm-art).
window.applyGlass = (g) => {
  const r = document.documentElement;
  glassTone = g ? g.tone : null; setLight();
  r.classList.toggle('glass', !!g);
  let a = document.getElementById('lm-art');
  if (!g) return a && a.remove();
  r.style.setProperty('--lm-tint', g.tint);
  if (!a) { a = document.createElement('div'); a.id = 'lm-art'; document.body.prepend(a); }
};
})();
