// Génère src/styles/_tokens.scss depuis design/tokens.json (export des variables
// Figma). Une règle par couple thème × accent : `[data-theme][data-accent]`,
// comme Charon. En sombre, l'accent repeint les fonds (rampe `elev`) ; en clair
// et en contraste, les neutres viennent du thème et l'accent ne donne que sa
// couleur.
'use strict';
const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..');
const tokens = JSON.parse(fs.readFileSync(path.join(root, 'design/tokens.json'), 'utf8'));
const THEMES = ['dark', 'light', 'contrast'];
const css = (name) => '--color-' + name.replace('/', '-');

function block(theme, accent) {
  const a = tokens.accent[accent];
  const c = tokens.color[theme];
  const v = {};
  if (theme === 'dark') {
    Object.assign(v, {
      'bg/app': a['elev/0'],
      'bg/popover': a['elev/1'],
      'bg/surface': a['elev/2'],
      'bg/raised': a['elev/3'],
      'bg/sunken': a['elev/sunken'],
      'border/default': a.border,
      'text/primary': a['text/1'],
      'text/emphasis': a['text/2'],
      'text/secondary': a['text/3'],
      'text/tertiary': a['text/4'],
      'text/muted': a['text/5'],
      'border/subtle': a['elev/3'],
    });
    Object.assign(v, {
      'accent/text': a['accent/bright'],
      'accent/fill': a['accent/solid'],
      'accent/fill-hover': a['accent/hover-dark'],
      'accent/deep': a['accent/deep'],
      'accent/ring': a['accent/ring-dark'],
    });
  } else if (theme === 'light') {
    Object.assign(v, {
      'accent/text': a['accent/deep'],
      'accent/fill': a['accent/solid'],
      'accent/fill-hover': a['accent/hover-light'],
      'accent/deep': a['accent/deep'],
      'accent/ring': a['accent/ring-light'],
    });
  } else {
    const b = a['accent/bright'];
    Object.assign(v, { 'accent/text': b, 'accent/fill': b, 'accent/fill-hover': b, 'accent/deep': b, 'accent/ring': a['accent/ring-dark'] });
  }
  for (const [k, val] of Object.entries(c)) {
    if (!(k in v)) v[k] = val;
  }
  return v;
}

let out = `// GÉNÉRÉ par scripts/make-tokens.cjs depuis design/tokens.json : ne pas éditer à la main.
// Les couleurs viennent des variables Figma (collections Color et Accent).
`;
for (const theme of THEMES) {
  for (const accent of Object.keys(tokens.accent)) {
    // Le thème sombre et l'accent par défaut valent aussi sans attribut d'accent.
    const sel = `[data-theme='${theme}'][data-accent='${accent}']` + (accent === 'kairos' ? `,\n[data-theme='${theme}']:not([data-accent])` : '');
    const vars = block(theme, accent);
    out +=
      `\n${sel} {\n` +
      Object.keys(vars)
        .sort()
        .map((k) => `  ${css(k)}: ${vars[k]};`)
        .join('\n') +
      '\n}\n';
  }
}
fs.writeFileSync(path.join(root, 'src/styles/_tokens.scss'), out);
console.log('src/styles/_tokens.scss écrit');
