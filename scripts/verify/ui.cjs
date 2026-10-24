// Les composants communs (src/app/ui), vérifiés par lecture du source :
// chacun existe, déclare ses entrées avec `input(` / `input.required(`, est autonome,
// en OnPush, avec un sélecteur `k-…`, et son style ne contient que des tokens
// (aucune couleur littérale ; polices et rayons en `var(--…)`).
'use strict';
const fs = require('fs');
const path = require('path');

const REPO = process.env.REPO ?? path.join(__dirname, '..', '..');
const UI = path.join(REPO, 'src', 'app', 'ui');

let fail = 0;
const check = (label, ok, extra) => {
  if (!ok) {
    fail++;
    console.log(`ÉCHEC  ${label}${extra ? ' :: ' + extra : ''}`);
  }
};
const read = (file) => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null);

// Les composants de la maquette, plus l'icône qu'ils partagent. `needs` : ce que le source doit contenir
// pour l'accessibilité clavier et lecteur d'écran.
const COMPONENTS = {
  icon: { needs: ['aria-hidden'] },
  button: { needs: ['type="button"', '[disabled]', ':hover', ':focus-visible', ':disabled'] },
  field: { needs: ['<label', '[for]', 'aria-invalid', 'aria-describedby', 'focus-ring', 'is-invalid'] },
  chip: { needs: ['aria-pressed', 'type="button"', ':hover', ':focus-visible'] },
  'recent-pill': { needs: ['type="button"', ':hover', ':focus-visible'] },
  'quick-resume': { needs: ['type="button"', ':hover', ':focus-visible'] },
  'section-title': { needs: [] },
  alert: { needs: ['role', 'alert'] },
  badge: { needs: [] },
  'popover-header': { needs: [] },
};

const index = read(path.join(UI, 'index.ts'));
check('src/app/ui/index.ts existe', index !== null);

for (const [name, { needs }] of Object.entries(COMPONENTS)) {
  const ts = read(path.join(UI, `${name}.ts`));
  const scss = read(path.join(UI, `${name}.scss`));
  check(`${name}.ts existe`, ts !== null);
  check(`${name}.scss existe`, scss !== null);
  if (ts === null || scss === null) continue;

  check(`${name} : sélecteur k-${name}`, new RegExp(`selector:\\s*'k-${name}'`).test(ts));
  check(`${name} : ChangeDetectionStrategy.OnPush`, ts.includes('ChangeDetectionStrategy.OnPush'));
  check(`${name} : pas de standalone: false`, !/standalone:\s*false/.test(ts));
  check(
    `${name} : entrées déclarées par input( ou input.required(`,
    /\binput(\.required)?(<[^>]*>)?\(/.test(ts) || /\bmodel(\.required)?(<[^>]*>)?\(/.test(ts),
  );
  check(`${name} : plus d'@Input() ni d'@Output()`, !/@(Input|Output)\(/.test(ts));
  for (const need of needs) {
    check(`${name} : contient « ${need} »`, ts.includes(need) || scss.includes(need));
  }
  if (index !== null) check(`${name} : exporté par index.ts`, index.includes(`./${name}'`));

  // Style : que des tokens.
  const noComments = scss.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
  check(`${name}.scss : aucune couleur littérale`, !/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/.test(noComments));
  for (const prop of ['font-size', 'border-radius']) {
    for (const m of noComments.matchAll(new RegExp(`(?<![\\w-])${prop}\\s*:\\s*([^;]+);`, 'g'))) {
      const value = m[1].trim();
      check(`${name}.scss : ${prop} vient d'un token`, /^var\(--[\w-]+\)$/.test(value) || value === 'inherit' || value === '0', value);
    }
  }
}

// Chaque composant du dossier doit être connu de ce contrôle.
if (fs.existsSync(UI)) {
  for (const file of fs.readdirSync(UI)) {
    const base = file.replace(/\.ts$/, '');
    if (file.endsWith('.ts') && base !== 'index' && !COMPONENTS[base]) check(`${file} est contrôlé`, false, 'à ajouter à COMPONENTS');
  }
}

if (fail) {
  console.log(`${fail} échec(s)`);
  process.exit(1);
}
console.log('ui : OK');
