// Dessine les icônes depuis le glyphe (src/assets/glyph.svg) : le master 1024
// de l'app et le gabarit monochrome de la barre de menus. Ensuite :
//   npx tauri icon src-tauri/icons/sources/icon-1024.png
// pour générer le jeu du bundle (icns, ico, tailles).
//
// macOS attend qu'une icône d'application occupe 824 pixels sur 1024 (80,5 %),
// avec 100 pixels de marge transparente : la marge se pose ICI, une fois.
// ICÔNE PROVISOIRE : un aplat Iris et le glyphe blanc, à remplacer par le
// dessin final quand il existera.
'use strict';
const fs = require('fs');
const path = require('path');
const { Resvg } = require('@resvg/resvg-js');

const root = path.join(__dirname, '..');
const glyph = fs.readFileSync(path.join(root, 'src/assets/glyph.svg'), 'utf8');
const paths = [...glyph.matchAll(/<path [^>]*\/>/g)].map((m) => m[0]);
const withFill = (fill) => paths.map((p) => p.replace('currentColor', fill)).join('');
const out = path.join(root, 'src-tauri/icons/sources');
fs.mkdirSync(out, { recursive: true });

const render = (svg, width, file) => fs.writeFileSync(path.join(out, file), new Resvg(svg, { fitTo: { mode: 'width', value: width } }).render().asPng());

// Master 1024 : carré arrondi Iris, 824 de côté, glyphe blanc de 440.
const scale = 440 / 154;
const tx = 512 - 220;
render(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">` +
    `<rect x="100" y="100" width="824" height="824" rx="185" fill="#8458ee"/>` +
    `<g transform="translate(${tx} ${tx}) scale(${scale})">${withFill('#ffffff')}</g></svg>`,
  1024,
  'icon-1024.png',
);

// Barre de menus : glyphe noir sur transparent, 44 px. Tauri ramène l'image à
// 18 pt de haut : toute marge ici rétrécit d'autant le glyphe (avec 9 px de
// marge il faisait 11 pt, bien plus petit que les autres icônes). On ne garde
// que 2 px de respiration. macOS le teinte selon le thème de la barre
// (icon_as_template).
render(
  `<svg xmlns="http://www.w3.org/2000/svg" width="44" height="44" viewBox="0 0 44 44">` +
    `<g transform="translate(2 2) scale(${40 / 154})">${withFill('#000000')}</g></svg>`,
  44,
  'tray-template.png',
);
console.log('icônes écrites dans src-tauri/icons/sources');
