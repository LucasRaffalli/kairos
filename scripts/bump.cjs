// Monte la version d'un coup : `npm run bump 0.2.0`.
//
// La version est écrite à TROIS endroits que check-release.cjs compare
// (package.json, tauri.conf.json, src-tauri/Cargo.toml), plus dans les deux
// crates du workspace pour qu'ils restent alignés. Le script les change tous,
// puis ajoute l'entrée du changelog en français ET en anglais.
//
// L'entrée est un GABARIT : son texte contient « À COMPLÉTER » / « TO COMPLETE »,
// et check-release.cjs refuse tant qu'il y reste. On ne publie donc jamais une
// release dont les notes n'ont pas été écrites.
'use strict';
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const root = path.join(__dirname, '..');
const file = (p) => path.join(root, p);
const SEMVER = /^\d+\.\d+\.\d+$/;
const PLACEHOLDER_FR = 'À COMPLÉTER';
const PLACEHOLDER_EN = 'TO COMPLETE';

const next = (process.argv[2] ?? '').replace(/^v/, '');
const current = JSON.parse(fs.readFileSync(file('package.json'), 'utf8')).version;

function fail(message) {
  console.error('✗ ' + message);
  process.exit(1);
}

const parts = (v) => v.split('.').map(Number);
function isNewer(a, b) {
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) {
    if (x[i] !== y[i]) return x[i] > y[i];
  }
  return false;
}

if (!SEMVER.test(next)) {
  fail(`Version invalide « ${process.argv[2] ?? ''} » : attendu X.Y.Z (par exemple 0.2.0). Version actuelle : ${current}.`);
}
if (!isNewer(next, current)) {
  fail(`${next} n'est pas plus récente que la version actuelle (${current}).`);
}

const changelogFr = JSON.parse(fs.readFileSync(file('src/assets/changelog.json'), 'utf8'));
if (changelogFr.some((e) => e.version === next)) {
  fail(`Le changelog a déjà une entrée pour ${next}.`);
}

// 1. JSON : package.json et tauri.conf.json (la mise en forme existante est gardée).
for (const p of ['package.json', 'src-tauri/tauri.conf.json']) {
  const raw = fs.readFileSync(file(p), 'utf8');
  const updated = raw.replace(/("version"\s*:\s*")[^"]+(")/, `$1${next}$2`);
  if (updated === raw) fail(`Version introuvable dans ${p}.`);
  fs.writeFileSync(file(p), updated);
}

// 2. Cargo.toml : la ligne `version` de la section [package].
for (const p of ['src-tauri/Cargo.toml', 'crates/kairos-core/Cargo.toml', 'crates/kairos-cli/Cargo.toml']) {
  const raw = fs.readFileSync(file(p), 'utf8');
  const updated = raw.replace(/^version\s*=\s*"[^"]+"/m, `version = "${next}"`);
  if (updated === raw) fail(`Version introuvable dans ${p}.`);
  fs.writeFileSync(file(p), updated);
}

// 3. Changelog : une entrée à compléter, en tête, FR et EN.
const today = new Date().toISOString().slice(0, 10);
changelogFr.unshift({
  version: next,
  date: today,
  title: PLACEHOLDER_FR,
  notes: [
    {
      kind: 'new',
      text: `${PLACEHOLDER_FR} : ce qui change pour l'utilisateur, en une ou deux phrases.`,
      how: `${PLACEHOLDER_FR} : comment l'essayer (facultatif, à supprimer sinon).`,
    },
  ],
});
fs.writeFileSync(file('src/assets/changelog.json'), JSON.stringify(changelogFr, null, 2) + '\n');

const en = JSON.parse(fs.readFileSync(file('src/assets/changelog.en.json'), 'utf8'));
const translated = {
  [next]: { title: PLACEHOLDER_EN, notes: [{ text: `${PLACEHOLDER_EN}: what changes for the user.`, how: `${PLACEHOLDER_EN}: how to try it.` }] },
  ...en,
};
fs.writeFileSync(file('src/assets/changelog.en.json'), JSON.stringify(translated, null, 2) + '\n');

// 4. Cargo.lock : suit les nouvelles versions des crates du workspace (hors ligne, instantané).
try {
  execFileSync('cargo', ['update', '--workspace', '--offline'], { cwd: root, stdio: 'ignore' });
} catch {
  console.log('(Cargo.lock non mis à jour : lance `cargo update --workspace` à la main)');
}

console.log(`✓ ${current} → ${next}`);
console.log('  versions : package.json, tauri.conf.json, src-tauri et crates/*/Cargo.toml');
console.log('  changelog : entrée à compléter dans src/assets/changelog.json (FR) et changelog.en.json (EN)');
console.log('Ensuite : écris les notes, commit, puis `npm run release`.');
