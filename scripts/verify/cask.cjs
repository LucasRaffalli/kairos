// La cask Homebrew, vérifiée sur le script RÉEL contre une fixture.
//
// make-cask.sh écrit la cask du tap : si la ligne `binary` manque, `brew install` ne met
// pas `kairos` dans le PATH, et le serveur MCP reste introuvable pour Claude.
'use strict';
const crypto = require('crypto');
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

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kairos-cask-'));
const write = (file, content) => {
  fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
  fs.writeFileSync(path.join(dir, file), content);
};
write('scripts/make-cask.sh', fs.readFileSync(path.join(REPO, 'scripts/make-cask.sh')));
write('src-tauri/tauri.conf.json', JSON.stringify({ version: '9.9.9' }));
const dmg = Buffer.from('faux dmg');
write('dist-macos/Kairos_9.9.9_aarch64.dmg', dmg);
fs.mkdirSync(path.join(dir, 'tap'));

const run = spawnSync('bash', [path.join(dir, 'scripts/make-cask.sh')], {
  encoding: 'utf8',
  env: { ...process.env, PUBLIC_URL: 'https://exemple.test/kairos', TAP_DIR: path.join(dir, 'tap') },
});
check('make-cask.sh : sortie 0', run.status === 0, run.stderr);
// Le gabarit est un heredoc non quoté : des backticks dans un commentaire seraient exécutés.
check("make-cask.sh : rien sur la sortie d'erreur", run.stderr === '', run.stderr);

const cask = fs.existsSync(path.join(dir, 'tap/Casks/kairos.rb')) ? fs.readFileSync(path.join(dir, 'tap/Casks/kairos.rb'), 'utf8') : '';
check('la cask est écrite', cask !== '');
check('version de tauri.conf.json', cask.includes('version "9.9.9"'));
check('sha256 du .dmg', cask.includes(`sha256 "${crypto.createHash('sha256').update(dmg).digest('hex')}"`));
check("l'app est toujours installée", cask.includes('app "Kairos.app"'));
// Littéral Ruby : `#{appdir}` est résolu par Homebrew, jamais par bash.
check("`kairos` est lié au PATH depuis l'app", cask.includes('binary "#{appdir}/Kairos.app/Contents/MacOS/kairos"'), cask);

if (fail) process.exit(1);
console.log('cask : OK');
