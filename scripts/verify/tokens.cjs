// Chaque variable de couleur utilisée dans le SCSS doit exister pour CHAQUE
// couple thème × accent. Une variable absente d'un seul couple ne casse rien :
// le popover s'affiche, simplement sans couleur, et personne ne le voit avant
// de basculer sur le bon accent.
'use strict';
const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..', '..');
const read = (p) => fs.readFileSync(path.join(root, p), 'utf8');
let fail = 0;
const check = (label, ok, extra) => {
  if (!ok) {
    fail++;
    console.log(`ÉCHEC  ${label}${extra ? ' :: ' + extra : ''}`);
  }
};

const tokens = JSON.parse(read('design/tokens.json'));
const themes = ['dark', 'light', 'contrast'];
const accents = Object.keys(tokens.accent);

// Les accents du code et ceux du design doivent être les mêmes.
const codeAccents = /ACCENTS = \[([^\]]*)\]/
  .exec(read('src/app/interfaces/appearance/accent.ts'))[1]
  .split(',')
  .map((v) => v.trim().replace(/'/g, ''));
check('accents du code = accents du design', JSON.stringify(codeAccents) === JSON.stringify(accents), `${codeAccents} / ${accents}`);

// Variables définies, par couple.
const scss = read('src/styles/_tokens.scss');
const blocks = [...scss.matchAll(/(\[data-theme='(\w+)'\]\[data-accent='(\w+)'\][^{]*)\{([^}]*)\}/g)];
const defined = new Map();
for (const [, , theme, accent, body] of blocks) {
  defined.set(`${theme}/${accent}`, new Set([...body.matchAll(/(--[\w-]+):/g)].map((m) => m[1])));
}
for (const t of themes) {
  for (const a of accents) {
    check(`bloc ${t} × ${a} présent`, defined.has(`${t}/${a}`));
  }
}

// Variables utilisées dans les feuilles de style de l'app.
function scssFiles(dir) {
  return fs.readdirSync(path.join(root, dir), { withFileTypes: true }).flatMap((e) => {
    const rel = path.join(dir, e.name);
    return e.isDirectory() ? scssFiles(rel) : rel.endsWith('.scss') && !rel.endsWith('_tokens.scss') ? [rel] : [];
  });
}
const colorVar = /var\((--color-[\w-]+)\)/g;
for (const file of scssFiles('src')) {
  for (const [, name] of read(file).matchAll(colorVar)) {
    for (const [key, vars] of defined) {
      check(`${name} défini pour ${key}`, vars.has(name), file);
    }
  }
}

// Le tokens.scss doit être à jour avec tokens.json.
const before = scss;
require('child_process').execFileSync('node', [path.join(__dirname, '..', 'make-tokens.cjs')], { stdio: 'ignore' });
check('_tokens.scss est à jour avec design/tokens.json', read('src/styles/_tokens.scss') === before, 'lancer npm run tokens');

if (fail) {
  console.log(`${fail} échec(s)`);
  process.exit(1);
}
console.log('tokens : OK');
