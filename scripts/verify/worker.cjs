// Le Worker des releases, vérifié sur le code RÉEL avec un faux bucket.
// Un mauvais comportement ici se voit chez l'utilisateur : une mise à jour qui
// échoue sans un mot, ou un chemin qui expose plus que prévu.
'use strict';
const fs = require('fs');
const path = require('path');
const esbuild = require(path.join(__dirname, '..', '..', 'node_modules', 'esbuild'));

let fail = 0;
const check = (label, ok, extra) => {
  if (!ok) {
    fail++;
    console.log(`ÉCHEC  ${label}${extra ? ' :: ' + extra : ''}`);
  }
};

const src = fs.readFileSync(path.join(__dirname, '..', '..', 'infra/release-worker/index.js'), 'utf8');
const { code } = esbuild.transformSync(src, { format: 'cjs', target: 'es2022' });
const mod = { exports: {} };
new Function("module", "exports", code)(mod, mod.exports);
const worker = mod.exports.default;

const files = {
  'latest.json': { body: '{"version":"0.1.0"}', type: 'text/plain' },
  'Kairos_0.1.0.app.tar.gz': { body: 'archive', type: 'application/gzip' },
};
const env = {
  RELEASES: {
    async get(key) {
      const f = files[key];
      if (!f) return null;
      return {
        body: f.body,
        etag: 'abc',
        httpEtag: '"abc"',
        writeHttpMetadata: (h) => h.set('Content-Type', f.type),
      };
    },
  },
};
const call = (method, url) => worker.fetch(new Request(`https://release.lucasraffalli.com${url}`, { method }), env);

(async () => {
  const manifest = await call('GET', '/kairos/latest.json');
  check('GET /kairos/latest.json : 200', manifest.status === 200);
  check('latest.json est servi en JSON', manifest.headers.get('Content-Type') === 'application/json');
  check('latest.json n\'est jamais mis en cache', manifest.headers.get('Cache-Control') === 'no-cache');
  check('et le corps est là', (await manifest.text()) === '{"version":"0.1.0"}');

  const archive = await call('GET', '/kairos/Kairos_0.1.0.app.tar.gz');
  check('une archive : 200 avec son ETag', archive.status === 200 && archive.headers.get('ETag') === '"abc"');
  check('une archive se met en cache une heure', archive.headers.get('Cache-Control') === 'public, max-age=3600');
  check('et garde son type', archive.headers.get('Content-Type') === 'application/gzip');

  const head = await call('HEAD', '/kairos/latest.json');
  check('HEAD : 200 sans corps', head.status === 200 && (await head.text()) === '');

  check('clé absente : 404', (await call('GET', '/kairos/absent.json')).status === 404);
  check('/kairos/ (listing) : 404', (await call('GET', '/kairos/')).status === 404);
  check('/kairos sans barre : 404', (await call('GET', '/kairos')).status === 404);
  check('.. : 404', (await call('GET', '/kairos/../latest.json')).status === 404);
  check('.. encodé : 404', (await call('GET', '/kairos/%2e%2e/x')).status === 404);
  check('// : 404', (await call('GET', '/kairos//latest.json')).status === 404);
  check('encodage invalide : 404', (await call('GET', '/kairos/%E0%A4%A')).status === 404);
  check('/charon/latest.json : 404 (jamais servi ici)', (await call('GET', '/charon/latest.json')).status === 404);
  check('POST : 405', (await call('POST', '/kairos/latest.json')).status === 405);
  check('PUT : 405', (await call('PUT', '/kairos/latest.json')).status === 405);
  check('404 sans cache', (await call('GET', '/kairos/absent')).headers.get('Cache-Control') === 'no-store');

  if (fail) {
    console.log(`${fail} échec(s)`);
    process.exit(1);
  }
  console.log('worker : OK');
})();
