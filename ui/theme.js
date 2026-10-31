// Tema: 'dark' | 'light' | 'auto' (sigue el modo de Windows). Pone o quita html.light.
window.applyTheme = (t) => {
  const mq = matchMedia('(prefers-color-scheme: dark)');
  const set = () => document.documentElement.classList.toggle('light', t === 'light' || (t !== 'dark' && !mq.matches));
  set();
  mq.onchange = set;
};
// Cristal: pone html.glass y el fondo con la carátula desenfocada (#lm-art). Apagado, no deja nada en la página.
window.applyGlass = (on, cover) => {
  document.documentElement.classList.toggle('glass', !!on);
  let a = document.getElementById('lm-art');
  if (!on) return a && a.remove();
  if (!a) { a = document.createElement('div'); a.id = 'lm-art'; a.innerHTML = '<img alt="">'; document.body.prepend(a); }
  const i = a.firstChild;
  if (cover && i.dataset.u !== cover) { i.dataset.u = cover; i.src = cover; }
  else if (!cover && i.dataset.u) { delete i.dataset.u; i.removeAttribute('src'); }
};
