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

// --- Contrastes et cascade (ronde de correction 1) --------------------------------------------
// Les valeurs viennent de _tokens.scss, par thème et par accent ; les pourcentages, du .scss lui-même.
const tokensCss = read(path.join(REPO, 'src', 'styles', '_tokens.scss')) ?? '';
const baseCss = read(path.join(REPO, 'src', 'styles', '_base.scss')) ?? '';
const palettes = {};
for (const m of tokensCss.matchAll(/\[data-theme='(\w+)'\]\[data-accent='(\w+)'\][^{]*\{([^}]*)\}/g)) {
  const t = {};
  for (const v of m[3].matchAll(/--color-([\w-]+):\s*(#[0-9a-f]{6})/g)) t[v[1]] = v[2];
  palettes[`${m[1]}/${m[2]}`] = t;
}
const rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const mix = (a, b, pct) => a.map((x, i) => (x * pct) / 100 + (b[i] * (100 - pct)) / 100);
const luminance = (c) => {
  const [r, g, b] = c.map((x) => {
    const v = x / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const ratio = (a, b) => {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};
const AA = 4.5;
const worst = (fn) => Math.min(...Object.entries(palettes).map(([key, p]) => fn(p, key.split('/')[0])));
check('les palettes de _tokens.scss sont lues', Object.keys(palettes).length === 15);
const src = (name, ext) => (read(path.join(UI, `${name}.${ext}`)) ?? '').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');

// Puce sélectionnée : teinte légère de l'accent sur la surface, contour d'accent, texte principal.
{
  const chip = src('chip', 'scss');
  const pressed = chip.indexOf(`&[aria-pressed='true']`);
  const dashed = chip.indexOf('&.dashed');
  const block = pressed >= 0 ? chip.slice(pressed, chip.indexOf('}', pressed)) : '';
  const tint = block.match(/background:\s*color-mix\(in srgb,\s*var\(--color-accent-text\)\s*(\d+)%,\s*var\(--color-bg-surface\)\)/);
  check('chip : fond sélectionné = teinte de l’accent sur la surface', tint !== null);
  check('chip : teinte entre 12 et 24 %', tint !== null && +tint[1] >= 12 && +tint[1] <= 24);
  check('chip : texte sélectionné en --color-text-primary', /color:\s*var\(--color-text-primary\)/.test(block));
  check('chip : contour sélectionné en --color-accent-text', /border-color:\s*var\(--color-accent-text\)/.test(block));
  if (tint !== null) {
    const w = worst((p) => ratio(rgb(p['text-primary']), mix(rgb(p['accent-text']), rgb(p['bg-surface']), +tint[1])));
    check(`chip : texte sélectionné ≥ ${AA}:1 (pire cas ${w.toFixed(2)})`, w >= AA);
  }
  // « + Autre » sélectionnée garde le style sélectionné : la règle sélectionnée vient APRÈS .dashed (même spécificité).
  check('chip : le style sélectionné l’emporte sur .dashed (ordre)', dashed >= 0 && pressed > dashed);
  check('chip : sélectionnée repasse le contour en plein', /border-style:\s*solid/.test(block));
}

// Titre d'étape (« ① Tâche parent », maquette 282:1560) : pastille ronde teintée de l'accent, contour
// d'accent, chiffre en texte principal (≥ 4,5:1 dans les trois thèmes), titre en gras et casse normale.
{
  const ts = read(path.join(UI, 'section-title.ts')) ?? '';
  const css = src('section-title', 'scss');
  check("section-title : entrée variant 'label' | 'step', 'label' par défaut", /variant\s*=\s*input<'label'\s*\|\s*'step'>\('label'\)/.test(ts));
  const start = css.indexOf(':host(.step)');
  const block = start >= 0 ? css.slice(start) : '';
  const badge = block.match(/\.step\s*\{([^}]*)\}/);
  const tint = badge && badge[1].match(/background:\s*color-mix\(in srgb,\s*var\(--color-accent-text\)\s*(\d+)%,\s*var\(--color-bg-popover\)\)/);
  check('section-title (étape) : pastille ronde', badge !== null && /border-radius:\s*var\(--radius-full\)/.test(badge[1]));
  check('section-title (étape) : pastille teintée de l’accent sur le popover', tint !== null);
  check('section-title (étape) : contour d’accent', badge !== null && /border:\s*1px solid var\(--color-accent-text\)/.test(badge[1]));
  check('section-title (étape) : chiffre en --color-text-primary', badge !== null && /(?<![\w-])color:\s*var\(--color-text-primary\)/.test(badge[1]));
  if (tint) {
    const w = worst((p) => ratio(rgb(p['text-primary']), mix(rgb(p['accent-text']), rgb(p['bg-popover']), +tint[1])));
    check(`section-title (étape) : chiffre ≥ ${AA}:1 (pire cas ${w.toFixed(2)})`, w >= AA);
  }
  const title = block.match(/\.title\s*\{([^}]*)\}/);
  check(
    'section-title (étape) : titre en gras, casse normale, texte principal',
    title !== null && /font-weight:\s*700/.test(title[1]) && /text-transform:\s*none/.test(title[1]) && /color:\s*var\(--color-text-primary\)/.test(title[1]),
  );
}

// Texte d'erreur : un token par thème, ≥ 4,5:1 sur la surface du champ et sur le fond de l'alerte.
{
  const defined = {};
  for (const m of baseCss.matchAll(/\[data-theme='(\w+)'\]\s*\{([^}]*)\}/g)) {
    const v = m[2].match(/--color-text-danger:\s*var\(--color-([\w-]+)\)/);
    if (v) defined[m[1]] = v[1];
  }
  check(
    '_base.scss : --color-text-danger défini pour dark, light et contrast',
    ['dark', 'light', 'contrast'].every((t) => defined[t]),
    JSON.stringify(defined),
  );
  const text = (p, theme) => rgb(p[defined[theme]] ?? '#ff00ff');
  check(`texte d’erreur sur la surface ≥ ${AA}:1`, worst((p, th) => ratio(text(p, th), rgb(p['bg-surface']))) >= AA);
  const alert = src('alert', 'scss');
  const tint = alert.match(/background:\s*color-mix\(in srgb,\s*var\(--tone\)\s*(\d+)%,\s*var\(--color-bg-popover\)\)/);
  check('alert : fond teinté lu', tint !== null);
  if (tint !== null)
    check(
      `glyphe d’erreur sur le fond de l’alerte ≥ ${AA}:1`,
      worst((p, th) => ratio(text(p, th), mix(rgb(p['status-danger']), rgb(p['bg-popover']), +tint[1]))) >= AA,
    );
  check('field : le message d’erreur utilise --color-text-danger', /\.error\s*\{[^}]*color:\s*var\(--color-text-danger\)/.test(src('field', 'scss')));
  check(
    'alert : le glyphe d’erreur utilise --color-text-danger',
    /--tone-text:\s*var\(--color-text-danger\)/.test(alert) && /\.glyph\s*\{[^}]*color:\s*var\(--tone-text\)/.test(alert),
  );
}

// Champ : désactivable, focalisable par le code, Entrée signalée ; le focus l'emporte sur le survol.
{
  const field = src('field', 'ts');
  check('field : entrée disabled', /readonly disabled = input\(/.test(field) && field.includes('[disabled]="disabled()"'));
  check('field : méthode publique focus()', /^\s*focus\(\):\s*void/m.test(field) && field.includes('.focus()'));
  check('field : sortie submitted sur Entrée', /readonly submitted = output</.test(field) && field.includes('(keydown.enter)'));
  const scss = src('field', 'scss');
  check('field : le survol ne masque pas le focus', /&:not\(:focus-within\):hover/.test(scss) && !/&:hover\s*\{/.test(scss));
  check('field : style désactivé', /:disabled/.test(scss));
}

// Bouton danger : survol par une couleur de token, pas par un filtre ; texte ≥ 4,5:1.
{
  const btn = src('button', 'scss');
  check('button : pas de filter au survol', !/filter\s*:/.test(btn));
  const hover = btn.match(
    /\.danger\s*\{[\s\S]*?&:hover:not\(:disabled\)\s*\{\s*background:\s*color-mix\(in srgb,\s*var\(--color-status-danger-fill\)\s*(\d+)%,\s*black\)/,
  );
  check('button : survol danger = color-mix du token de fond', hover !== null);
  if (hover !== null) {
    const w = worst((p) => ratio(rgb(p['text-on-accent']), mix(rgb(p['status-danger-fill']), [0, 0, 0], +hover[1])));
    check(`button : texte du danger survolé ≥ ${AA}:1 (pire cas ${w.toFixed(2)})`, w >= AA);
  }
}

// Texte de succès (durée du lot en cours, liste du jour) : un token par thème, ≥ 4,5:1 sur la surface.
// Le vert `status-success` est le même dans les thèmes sombre et clair : sur le clair, il tombe à 1,9:1.
{
  const defs = {};
  for (const m of baseCss.matchAll(/\[data-theme='(\w+)'\]\s*\{([^}]*)\}/g)) {
    const v = m[2].match(/--color-text-success:\s*([^;]+);/);
    if (v) defs[m[1]] = v[1].trim();
  }
  check(
    '_base.scss : --color-text-success défini pour dark, light et contrast',
    ['dark', 'light', 'contrast'].every((t) => defs[t]),
    JSON.stringify(defs),
  );
  const resolve = (p, value) => {
    const plain = value?.match(/^var\(--color-([\w-]+)\)$/);
    if (plain) return rgb(p[plain[1]] ?? '#ff00ff');
    const mixed = value?.match(/^color-mix\(in srgb,\s*var\(--color-([\w-]+)\)\s*(\d+)%,\s*var\(--color-([\w-]+)\)\)$/);
    if (mixed) return mix(rgb(p[mixed[1]]), rgb(p[mixed[3]]), +mixed[2]);
    return rgb('#ff00ff');
  };
  const w = worst((p, th) => ratio(resolve(p, defs[th]), rgb(p['bg-surface'])));
  check(`texte de succès sur la surface ≥ ${AA}:1 (pire cas ${w.toFixed(2)})`, w >= AA);
  const today = (read(path.join(REPO, 'src', 'app', 'features', 'popover', 'today-list.scss')) ?? '').replace(/^\s*\/\/.*$/gm, '');
  check(
    'today-list : la durée en cours utilise --color-text-success',
    /color:\s*var\(--color-text-success\)/.test(today) && !/--color-status-success/.test(today),
  );
}

// Mesures des maquettes 220:81 et 220:179 (tâche 7) : hauteurs des contrôles, tailles et graisses de texte.
{
  const block = (css, selector) => {
    const m = css.match(new RegExp(`(?:^|\\n)\\s*${selector}\\s*\\{([^}]*)\\}`));
    return m ? m[1] : '';
  };
  const field = src('field', 'scss');
  check('field : libellé en graisse moyenne (500)', /font-weight:\s*500/.test(block(field, '\\.label')));
  check('field : champ de 40 px de haut', /min-height:\s*40px/.test(block(field, '\\.box')));
  const btn = src('button', 'scss');
  check('button : 40 px de haut', /min-height:\s*40px/.test(btn.slice(btn.indexOf('.btn {'))));
  check('button : icône seule, un carré de 40 px', /&:not\(:has\(\.label:not\(:empty\)\)\)\s*\{[^}]*width:\s*40px/.test(btn));
  check('chip : 30 px de haut', /min-height:\s*30px/.test(block(src('chip', 'scss'), '\\.chip')));
  check('recent-pill : texte de 9 px (--font-size-2xs)', /font-size:\s*var\(--font-size-2xs\)/.test(block(src('recent-pill', 'scss'), '\\.pill')));
  check(
    'quick-resume : triangle de lecture plein',
    /<k-icon class="play filled"/.test(read(path.join(UI, 'quick-resume.ts')) ?? '') &&
      /:host\(\.filled\)\s+svg\s*\{[^}]*fill:\s*currentColor/.test(src('icon', 'scss')),
  );
  check('section-title (étape) : rangée de 20 px', /:host\(\.step\)\s*\{[^}]*min-height:\s*20px/.test(src('section-title', 'scss')));
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
