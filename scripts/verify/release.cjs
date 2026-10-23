// La cohérence d'une release, vérifiée sur le script RÉEL contre des fixtures.
//
// check-release.cjs tourne dans la CI avant le moindre build de 20 minutes :
// une release incohérente (version, changelog, traduction, tag) doit échouer
// en quelques secondes, avec un message qui dit quoi corriger.
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

function fixture({ pkg = '0.1.0', tauri = '0.1.0', cargo = '0.1.0', fr, en }) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kairos-release-'));
  const write = (file, content) => {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.writeFileSync(path.join(dir, file), content);
  };
  write('scripts/check-release.cjs', fs.readFileSync(path.join(REPO, 'scripts/check-release.cjs')));
  write('package.json', JSON.stringify({ version: pkg }));
  write('src-tauri/tauri.conf.json', JSON.stringify({ version: tauri }));
  write('src-tauri/Cargo.toml', `[package]\nname = "kairos-app"\nversion = "${cargo}"\n`);
  write('src/assets/changelog.json', JSON.stringify(fr ?? [{ version: '0.1.0', date: '2026-10-23', title: 'a', notes: [{ kind: 'new', text: 'x' }] }]));
  write('src/assets/changelog.en.json', JSON.stringify(en ?? { '0.1.0': { title: 'a', notes: [{ text: 'x' }] } }));
  return dir;
}
const run = (dir, ...args) => spawnSync('node', [path.join(dir, 'scripts/check-release.cjs'), ...args], { encoding: 'utf8' });

// 1. Le cas sain, avec et sans tag.
{
  const ok = run(fixture({}));
  check('release cohérente : sortie 0', ok.status === 0, ok.stderr);
  check('release cohérente avec son tag', run(fixture({}), 'v0.1.0').status === 0);
}
// 2. Les trois versions doivent coïncider.
{
  const r = run(fixture({ tauri: '0.2.0' }));
  check('versions divergentes : sortie 1', r.status === 1);
  check('et le message nomme les fichiers', /Versions différentes/.test(r.stderr) && /tauri\.conf\.json=0\.2\.0/.test(r.stderr), r.stderr);
}
// 3. Le changelog doit avoir une entrée pour la version.
{
  const r = run(
    fixture({ fr: [{ version: '0.0.9', date: 'd', title: 't', notes: [{ kind: 'new', text: 'x' }] }], en: { '0.0.9': { notes: [{ text: 'x' }] } } }),
  );
  check('sans entrée de changelog : sortie 1', r.status === 1 && /pas d'entrée pour la version 0\.1\.0/.test(r.stderr), r.stderr);
}
// 4. La traduction anglaise doit exister, avec le même nombre de notes.
{
  const noEn = run(fixture({ en: {} }));
  check('sans traduction EN : sortie 1', noEn.status === 1 && /pas de traduction pour la version 0\.1\.0/.test(noEn.stderr), noEn.stderr);
  const fr2 = [
    {
      version: '0.1.0',
      date: 'd',
      title: 't',
      notes: [
        { kind: 'new', text: 'a' },
        { kind: 'fixed', text: 'b' },
      ],
    },
  ];
  const fewer = run(fixture({ fr: fr2, en: { '0.1.0': { title: 't', notes: [{ text: 'a' }] } } }));
  check('moins de notes en EN : sortie 1', fewer.status === 1 && /1 note\(s\) traduite\(s\) pour 2 en français/.test(fewer.stderr), fewer.stderr);
  const blank = run(fixture({ en: { '0.1.0': { title: 'a', notes: [{ text: '  ' }] } } }));
  check('note EN vide : sortie 1', blank.status === 1 && /pas de texte/.test(blank.stderr), blank.stderr);
}
// 5. Le tag doit valoir v<version>.
{
  const r = run(fixture({}), 'v0.2.0');
  check('mauvais tag : sortie 1', r.status === 1 && /ne correspond pas à la version 0\.1\.0/.test(r.stderr), r.stderr);
}
// 5 bis. Un gabarit de `npm run bump` ne part jamais en release.
{
  const r = run(fixture({ fr: [{ version: '0.1.0', date: 'd', title: 'À COMPLÉTER', notes: [{ kind: 'new', text: 'x' }] }] }));
  check('gabarit FR laissé : sortie 1', r.status === 1 && /À COMPLÉTER/.test(r.stderr), r.stderr);
  const e = run(fixture({ en: { '0.1.0': { title: 'TO COMPLETE', notes: [{ text: 'x' }] } } }));
  check('gabarit EN laissé : sortie 1', e.status === 1 && /TO COMPLETE/.test(e.stderr), e.stderr);
}
// 6. Le dépôt courant, lui, est cohérent.
{
  const r = spawnSync('node', [path.join(REPO, 'scripts/check-release.cjs')], { encoding: 'utf8' });
  check('le dépôt courant est cohérent', r.status === 0, r.stderr);
}

if (fail) {
  console.log(`${fail} échec(s)`);
  process.exit(1);
}
console.log('release : OK');
