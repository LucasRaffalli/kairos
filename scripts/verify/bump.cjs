// `npm run bump` : il touche cinq fichiers de version et deux changelogs. Une
// erreur ici se paie à la release (versions qui divergent, notes oubliées), donc
// le script RÉEL est éprouvé contre une copie jetable du dépôt.
'use strict';
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');

const REPO = path.join(__dirname, '..', '..');
let fail = 0;
const check = (label, ok, extra) => {
  if (!ok) {
    fail++;
    console.log(`ÉCHEC  ${label}${extra ? ' :: ' + extra : ''}`);
  }
};

function fixture(version) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kairos-bump-'));
  const write = (file, content) => {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.writeFileSync(path.join(dir, file), content);
  };
  for (const s of ['bump.cjs', 'check-release.cjs']) write(`scripts/${s}`, fs.readFileSync(path.join(REPO, 'scripts', s)));
  write('package.json', JSON.stringify({ name: 'kairos', version }, null, 2) + '\n');
  write('src-tauri/tauri.conf.json', JSON.stringify({ productName: 'Kairos', version }, null, 2) + '\n');
  for (const crate of ['src-tauri', 'crates/kairos-core', 'crates/kairos-cli'])
    write(`${crate}/Cargo.toml`, `[package]\nname = "x"\nversion = "${version}"\nedition = "2021"\n\n[dependencies]\nserde = { version = "1" }\n`);
  write('src/assets/changelog.json', JSON.stringify([{ version, date: '2026-10-23', title: 'a', notes: [{ kind: 'new', text: 'x' }] }], null, 2));
  write('src/assets/changelog.en.json', JSON.stringify({ [version]: { title: 'a', notes: [{ text: 'x' }] } }, null, 2));
  return dir;
}
const bump = (dir, ...args) => spawnSync('node', [path.join(dir, 'scripts/bump.cjs'), ...args], { encoding: 'utf8' });
const checkRelease = (dir) => spawnSync('node', [path.join(dir, 'scripts/check-release.cjs')], { encoding: 'utf8' });
const read = (dir, p) => fs.readFileSync(path.join(dir, p), 'utf8');

// 1. Les versions refusées ne touchent à rien.
{
  const dir = fixture('0.1.0');
  for (const arg of [undefined, 'abc', '1.0', '1.0.0.0']) {
    const r = arg === undefined ? bump(dir) : bump(dir, arg);
    check(`« ${arg} » refusé`, r.status === 1 && /Version invalide/.test(r.stderr), r.stderr);
  }
  check('la même version est refusée', /pas plus récente/.test(bump(dir, '0.1.0').stderr));
  check('une version plus ancienne est refusée', bump(dir, '0.0.9').status === 1);
  check("rien n'a été modifié", read(dir, 'package.json').includes('"0.1.0"') && JSON.parse(read(dir, 'src/assets/changelog.json')).length === 1);
}
// 2. Une montée de version change TOUT, et le gabarit bloque la release.
{
  const dir = fixture('0.1.0');
  const r = bump(dir, 'v0.2.0');
  check('bump 0.2.0 (avec le v) : sortie 0', r.status === 0, r.stderr);
  for (const p of ['package.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'crates/kairos-core/Cargo.toml', 'crates/kairos-cli/Cargo.toml']) {
    check(`${p} est à 0.2.0`, /0\.2\.0/.test(read(dir, p)) && !/0\.1\.0/.test(read(dir, p)), p);
  }
  check("la dépendance serde n'est pas touchée", /serde = \{ version = "1" \}/.test(read(dir, 'src-tauri/Cargo.toml')));
  const fr = JSON.parse(read(dir, 'src/assets/changelog.json'));
  check("le changelog FR a l'entrée en tête", fr[0].version === '0.2.0' && fr.length === 2);
  check("et l'ancienne est intacte", fr[1].version === '0.1.0');
  const en = JSON.parse(read(dir, 'src/assets/changelog.en.json'));
  check('le changelog EN a la clé, avec autant de notes', en['0.2.0']?.notes.length === fr[0].notes.length);
  const blocked = checkRelease(dir);
  check('check-release refuse le gabarit', blocked.status === 1 && /À COMPLÉTER/.test(blocked.stderr), blocked.stderr);
  // Notes écrites : la release passe.
  fr[0] = { ...fr[0], title: 'Le chrono', notes: [{ kind: 'new', text: 'Un chrono.' }] };
  fs.writeFileSync(path.join(dir, 'src/assets/changelog.json'), JSON.stringify(fr));
  en['0.2.0'] = { title: 'The timer', notes: [{ text: 'A timer.' }] };
  fs.writeFileSync(path.join(dir, 'src/assets/changelog.en.json'), JSON.stringify(en));
  const ok = checkRelease(dir);
  check('notes écrites : check-release passe', ok.status === 0, ok.stderr);
  check('relancer le même bump est refusé', bump(dir, '0.2.0').status === 1);
}
// 3. La comparaison est numérique : 0.10.0 vient après 0.9.0.
{
  const dir = fixture('0.9.0');
  check('0.9.0 → 0.10.0 accepté', bump(dir, '0.10.0').status === 0);
}

if (fail) {
  console.log(`${fail} échec(s)`);
  process.exit(1);
}
console.log('bump : OK');
