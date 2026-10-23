// Mises à jour et nouveautés, vérifiées sur le code COMPILÉ.
//
// Rien de cela ne se voit en développement : l'annonce ne part qu'une fois par
// version, et la vérification de mise à jour échoue (hors ligne, `latest.json`
// absent) justement quand personne ne regarde. Une régression passerait
// inaperçue jusqu'à la première vraie release.
'use strict';
const fs = require('fs');
const path = require('path');

const REPO = process.env.REPO ?? path.join(__dirname, '..', '..');
const esbuild = require(path.join(REPO, 'node_modules', 'esbuild'));

let fail = 0;
const check = (label, ok, extra) => {
  if (!ok) {
    fail++;
    console.log(`ÉCHEC  ${label}${extra ? ' :: ' + extra : ''}`);
  }
};

function load(file, req) {
  const src = fs.readFileSync(path.join(REPO, file), 'utf8');
  const { code } = esbuild.transformSync(src, {
    loader: 'ts',
    format: 'cjs',
    target: 'es2022',
    tsconfigRaw: { compilerOptions: { experimentalDecorators: true } },
  });
  const module = { exports: {} };
  new Function('require', 'module', 'exports', code)(req, module, module.exports);
  return module.exports;
}

const NG = {
  Injectable: () => (t) => t,
  signal(v) {
    const s = () => v;
    s.set = (x) => (v = x);
    s.asReadonly = () => s;
    return s;
  },
  computed: (fn) => fn,
};

// --- 1. Le journal : tri, traduction, appariement ---------------------------
const i18n = load('src/app/services/system/changelog-i18n.ts', () => ({}));
const entry = (version) => ({ version, date: '2026-01-01', title: `t${version}`, notes: [{ kind: 'new', text: `fr ${version}` }] });
const sorted = i18n.newestFirst([entry('0.9.0'), entry('0.10.0'), entry('0.2.1')]).map((e) => e.version);
check('0.10.0 passe avant 0.9.0', sorted.join() === '0.10.0,0.9.0,0.2.1', sorted.join());
const tr = i18n.localizeChangelog([entry('1.0.0')], { '1.0.0': { title: 'T', notes: [{ text: 'en' }] } });
check('la traduction remplace le texte', tr[0].notes[0].text === 'en' && tr[0].title === 'T');
const missing = i18n.localizeChangelog([entry('1.0.0')], {});
check('sans traduction : le français reste', missing[0].notes[0].text === 'fr 1.0.0');
check('pas de traduction du tout : inchangé', i18n.localizeChangelog([entry('1.0.0')], null)[0].notes[0].text === 'fr 1.0.0');

// --- 2. Quand annoncer les nouveautés ---------------------------------------
check('jamais au tout premier lancement', i18n.shouldAnnounce('0.1.0', null) === false);
check('une fois la version changée', i18n.shouldAnnounce('0.2.0', '0.1.0') === true);
check('pas deux fois la même version', i18n.shouldAnnounce('0.2.0', '0.2.0') === false);

// --- 3. Le vérificateur ne lève jamais --------------------------------------
async function updaterWith(checkImpl) {
  globalThis.setTimeout = () => 0;
  globalThis.setInterval = () => 0;
  const mod = load('src/app/services/system/updater.service.ts', (spec) => {
    if (spec === '@angular/core') return NG;
    if (spec === '@tauri-apps/api/app') return { getVersion: async () => '0.1.0' };
    if (spec === '@tauri-apps/plugin-process') return { relaunch: async () => {} };
    if (spec === '@tauri-apps/plugin-updater') return { check: checkImpl };
    return {};
  });
  return new mod.UpdaterService();
}

(async () => {
  const offline = await updaterWith(async () => {
    throw new Error('réseau injoignable');
  });
  let threw = false;
  let result;
  try {
    result = await offline.check();
  } catch {
    threw = true;
  }
  check('hors ligne : check() ne lève pas', !threw);
  check('hors ligne : le résultat est une erreur lisible', result?.kind === 'error' && result.message.includes('réseau'));

  const silent = await updaterWith(async () => {
    throw new Error('404');
  });
  await silent.check(true);
  check("contrôle silencieux en échec : rien ne s'affiche", silent.status().kind === 'idle');

  const none = await updaterWith(async () => null);
  check('pas de mise à jour : none', (await none.check()).kind === 'none');

  const found = await updaterWith(async () => ({ version: '0.2.0' }));
  const status = await found.check();
  check('mise à jour trouvée : available avec la version', status.kind === 'available' && status.version === '0.2.0');
  check('et elle est signalée', found.updateAvailable() === true);

  // --- 4. Le changelog du dépôt : FR et EN appariés ------------------------
  const fr = JSON.parse(fs.readFileSync(path.join(REPO, 'src/assets/changelog.json'), 'utf8'));
  const en = JSON.parse(fs.readFileSync(path.join(REPO, 'src/assets/changelog.en.json'), 'utf8'));
  for (const e of fr) {
    check(`changelog ${e.version} : traduit avec le même nombre de notes`, en[e.version]?.notes?.length === e.notes.length);
  }

  if (fail) {
    console.log(`${fail} échec(s)`);
    process.exit(1);
  }
  console.log('whats-new : OK');
})();
