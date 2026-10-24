// Le paquet .mcpb de Claude Desktop, vérifié sur le script RÉEL contre des fixtures.
//
// make-mcpb.sh extrait le binaire `kairos` de l'archive de l'app et l'emballe avec un
// manifeste. Un paquet sans binaire, ou d'une autre version que le tag, ne doit jamais
// partir sur R2 : les deux cas échouent ici, avec un message qui dit quoi corriger.
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
const sh = (cmd, args, opts = {}) => spawnSync(cmd, args, { encoding: 'utf8', ...opts });

// Une fixture : le dépôt en miniature, avec une archive d'app dont le binaire est un faux
// `kairos` (script shell) qui annonce la version voulue. `withBinary: false` : archive sans lui.
function fixture({ binaryVersion = '9.9.9', withBinary = true } = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kairos-mcpb-'));
  const write = (file, content, mode) => {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.writeFileSync(path.join(dir, file), content, { mode });
  };
  write('scripts/make-mcpb.sh', fs.readFileSync(path.join(REPO, 'scripts/make-mcpb.sh')));
  write('packaging/mcpb/manifest.json', fs.readFileSync(path.join(REPO, 'packaging/mcpb/manifest.json')));
  write('packaging/mcpb/start.sh', fs.readFileSync(path.join(REPO, 'packaging/mcpb/start.sh')));
  write('src-tauri/tauri.conf.json', JSON.stringify({ version: '9.9.9' }));
  write('src-tauri/icons/128x128.png', 'png');
  const app = path.join(dir, 'app/Kairos.app/Contents/MacOS');
  fs.mkdirSync(app, { recursive: true });
  fs.writeFileSync(path.join(app, 'kairos-app'), 'x', { mode: 0o755 });
  if (withBinary) {
    fs.writeFileSync(path.join(app, 'kairos'), `#!/bin/sh\necho "kairos ${binaryVersion}"\n`, { mode: 0o755 });
  }
  fs.mkdirSync(path.join(dir, 'dist-macos'));
  const tar = sh('tar', ['-czf', path.join(dir, 'dist-macos/Kairos.app.tar.gz'), '-C', path.join(dir, 'app'), 'Kairos.app']);
  if (tar.status !== 0) throw new Error(tar.stderr);
  return dir;
}
const make = (dir, ...args) => sh('bash', [path.join(dir, 'scripts/make-mcpb.sh'), ...args], { cwd: dir });

// 1. Le cas sain.
{
  const dir = fixture();
  const out = path.join(dir, 'out');
  const run = make(dir, out);
  check('assemblage : sortie 0', run.status === 0, run.stderr);
  const pkg = path.join(out, 'Kairos_9.9.9.mcpb');
  check('le paquet est nommé Kairos_<version>.mcpb', fs.existsSync(pkg));
  if (fs.existsSync(pkg)) {
    const listing = sh('unzip', ['-Z', pkg]).stdout;
    check('manifest.json présent', /\smanifest\.json$/m.test(listing), listing);
    check('icon.png présent', /\sicon\.png$/m.test(listing), listing);
    check('server/kairos est exécutable dans le zip', /^-rwx\S+\s.*server\/kairos$/m.test(listing), listing);
    const manifest = JSON.parse(sh('unzip', ['-p', pkg, 'manifest.json']).stdout);
    check('la version du manifeste vient de tauri.conf.json', manifest.version === '9.9.9', JSON.stringify(manifest));
    check('le manifeste vise le binaire', manifest.server.entry_point === 'server/kairos');
    check(
      'le manifeste passe par le script de démarrage',
      manifest.server.mcp_config.command === '/bin/sh' && manifest.server.mcp_config.args[0].endsWith('/server/start.sh'),
      JSON.stringify(manifest.server),
    );
    check('server/start.sh est dans le zip', /\sserver\/start\.sh$/m.test(listing), listing);
    // Le script de démarrage lève la quarantaine du binaire (sans quoi macOS le tue : code 137)
    // puis le lance. Essai sur le paquet défait, avec la quarantaine posée.
    const unpacked = path.join(dir, 'unpacked');
    sh('unzip', ['-q', pkg, '-d', unpacked]);
    // `xattr` n'existe que sur macOS : la CI Linux (ci.yml) lance aussi ce script.
    const mac = process.platform === 'darwin';
    if (mac) sh('xattr', ['-w', 'com.apple.quarantine', '0081;00000000;Safari;', path.join(unpacked, 'server/kairos')]);
    const started = sh('/bin/sh', [path.join(unpacked, 'server/start.sh')]);
    check('start.sh lance le binaire', started.stdout.trim() === 'kairos 9.9.9', started.stdout + started.stderr);
    if (mac) check('start.sh lève la quarantaine', !sh('xattr', [path.join(unpacked, 'server/kairos')]).stdout.includes('com.apple.quarantine'));
    check('aucun gabarit __VERSION__ ne reste', !JSON.stringify(manifest).includes('__VERSION__'));
    // 2. --check : la bonne version passe, une autre échoue.
    const good = make(dir, '--check', pkg, '9.9.9');
    check('--check : bonne version, sortie 0', good.status === 0, good.stderr);
    const bad = make(dir, '--check', pkg, '1.2.3');
    check('--check : autre version, sortie 1', bad.status === 1, bad.stdout + bad.stderr);
    check('--check : le message nomme les deux versions', /9\.9\.9/.test(bad.stderr) && /1\.2\.3/.test(bad.stderr), bad.stderr);
  }
}

// 3. Une archive sans binaire : échec clair, aucun paquet écrit.
{
  const dir = fixture({ withBinary: false });
  const out = path.join(dir, 'out');
  const run = make(dir, out);
  check('archive sans kairos : sortie 1', run.status === 1, run.stdout + run.stderr);
  check('le message nomme le binaire manquant', /Contents\/MacOS\/kairos/.test(run.stderr), run.stderr);
  check('aucun .mcpb écrit', !fs.existsSync(out) || fs.readdirSync(out).length === 0);
}

// 4. Pas d'archive du tout.
{
  const dir = fixture();
  fs.rmSync(path.join(dir, 'dist-macos/Kairos.app.tar.gz'));
  const run = make(dir, path.join(dir, 'out'));
  check('aucune archive : sortie 1', run.status === 1, run.stderr);
  check('le message nomme dist-macos', /dist-macos/.test(run.stderr), run.stderr);
}

if (fail) process.exit(1);
console.log('mcpb : OK');
