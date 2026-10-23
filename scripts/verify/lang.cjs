// Le français et l'anglais doivent avoir exactement les mêmes clés.
'use strict';
const fs = require('fs');
const path = require('path');

const dir = path.join(__dirname, '..', '..', 'src/app/lang');
const keys = (file) => [...fs.readFileSync(path.join(dir, file), 'utf8').matchAll(/^\s*'([\w.]+)':/gm)].map((m) => m[1]).sort();
const fr = keys('fr.ts');
const en = keys('en.ts');
const missing = fr.filter((k) => !en.includes(k)).concat(en.filter((k) => !fr.includes(k)));
if (!fr.length || missing.length) {
  console.log(`ÉCHEC  clés différentes entre fr.ts et en.ts :: ${missing.join(', ') || 'aucune clé lue'}`);
  process.exit(1);
}
console.log('lang : OK');
