// Tema: 'dark' | 'light' | 'auto' (sigue el modo de Windows). Pone o quita html.light.
window.applyTheme = (t) => {
  const mq = matchMedia('(prefers-color-scheme: dark)');
  const set = () => document.documentElement.classList.toggle('light', t === 'light' || (t !== 'dark' && !mq.matches));
  set();
  mq.onchange = set;
};
