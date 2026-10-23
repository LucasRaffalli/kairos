// Le thème, vérifié sur le code COMPILÉ : la préférence, ce qui est peint, et
// le suivi du système. `data-theme` doit rester un thème RÉEL : les accents
// s'y accrochent, et un attribut valant « auto » ne matcherait aucune règle.
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

function freshWorld(stored, systemLight) {
  const effects = [];
  const NG = {
    signal(v) {
      const s = () => v;
      s.set = (x) => (v = x);
      s.asReadonly = () => () => v;
      return s;
    },
    computed: (fn) => fn,
    effect: (fn) => {
      effects.push(fn);
      fn();
      return { destroy() {} };
    },
    inject: () => doc,
    Injectable: () => (target) => target,
    DOCUMENT: 'document',
  };
  const listeners = [];
  const media = { matches: systemLight, addEventListener: (_t, fn) => listeners.push(fn) };
  const doc = { documentElement: { dataset: {} }, defaultView: { matchMedia: () => media } };
  const store = new Map(Object.entries(stored ?? {}));
  global.localStorage = {
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => store.set(k, String(v)),
  };

  const src = fs.readFileSync(path.join(REPO, 'src/app/services/appearance/theme.service.ts'), 'utf8');
  const { code } = esbuild.transformSync(src, {
    loader: 'ts',
    format: 'cjs',
    target: 'es2022',
    tsconfigRaw: { compilerOptions: { experimentalDecorators: true } },
  });
  const module = { exports: {} };
  const list = (file, name) => {
    const raw = fs.readFileSync(path.join(REPO, `src/app/interfaces/appearance/${file}`), 'utf8');
    return new RegExp(`${name} = \\[([^\\]]*)\\]`)
      .exec(raw)[1]
      .split(',')
      .map((v) => v.trim().replace(/'/g, ''))
      .filter(Boolean);
  };
  const req = (spec) => {
    if (spec === '@angular/core') return NG;
    if (spec === '@app/interfaces') {
      const THEMES = list('theme.ts', 'THEMES');
      const ACCENTS = list('accent.ts', 'ACCENTS');
      return { isThemePreference: (v) => v === 'auto' || THEMES.includes(v), isAccent: (v) => ACCENTS.includes(v) };
    }
    return {};
  };
  new Function('require', 'module', 'exports', code)(req, module, module.exports);
  const service = new module.exports.ThemeService();
  const flush = () => effects.forEach((fn) => fn());
  flush();
  return {
    service,
    doc,
    store,
    flush,
    systemGoes: (light) => {
      media.matches = light;
      listeners.forEach((fn) => fn());
      flush();
    },
  };
}

// 1. Rien d'enregistré : on suit le système.
{
  const w = freshWorld(null, true);
  check('sans réglage : automatique', w.service.preference() === 'auto');
  check('le clair du système est peint', w.service.theme() === 'light');
  check('data-theme reçoit un thème RÉEL', w.doc.documentElement.dataset.theme === 'light');
  check("l'accent par défaut est kairos", w.doc.documentElement.dataset.accent === 'kairos');
}
// 2. Le système bascule, l'application suit ; la préférence ne bouge pas.
{
  const w = freshWorld(null, true);
  w.systemGoes(false);
  check('passage au sombre suivi', w.service.theme() === 'dark' && w.doc.documentElement.dataset.theme === 'dark');
  check('la préférence reste auto', w.service.preference() === 'auto');
}
// 3. C'est la PRÉFÉRENCE qui est enregistrée, pas la couleur du moment.
{
  const w = freshWorld(null, false);
  check('« auto » est retenu tel quel', w.store.get('kairos:theme') === 'auto');
  w.service.select('light');
  w.flush();
  check('un choix explicite est retenu et peint', w.store.get('kairos:theme') === 'light' && w.doc.documentElement.dataset.theme === 'light');
  w.systemGoes(false);
  check('le système ne contredit plus un choix explicite', w.service.theme() === 'light');
}
// 4. Ce qui était enregistré est relu ; une valeur inconnue ne casse rien.
{
  check('un thème choisi revient', freshWorld({ 'kairos:theme': 'contrast' }, true).service.theme() === 'contrast');
  const bad = freshWorld({ 'kairos:theme': 'chartreuse', 'kairos:accent': 'rose', 'kairos:secrets': '{pas du json' }, false);
  check('valeur inconnue : retour à automatique', bad.service.preference() === 'auto');
  check('accent inconnu : kairos', bad.service.accent() === 'kairos');
  check('secrets illisibles : aucun accent caché', !bad.service.visibleAccents().some((o) => o.value === 'unicorn'));
}
// 5. L'accent caché.
{
  const w = freshWorld(null, false);
  check('unicorn est absent de la liste au départ', !w.service.visibleAccents().some((o) => o.value === 'unicorn'));
  w.service.selectAccent('unicorn');
  check('on ne peut pas le choisir avant de le trouver', w.service.accent() === 'kairos');
  w.service.activateSecretAccent();
  w.flush();
  check('le code le pose', w.service.accent() === 'unicorn' && w.doc.documentElement.dataset.accent === 'unicorn');
  w.service.selectAccent('jade');
  check('on peut en sortir', w.service.accent() === 'jade');
  check(
    'il reste listé une fois trouvé',
    w.service.visibleAccents().some((o) => o.value === 'unicorn'),
  );
  w.service.selectAccent('unicorn');
  check('et on peut y revenir', w.service.accent() === 'unicorn');
  check('la découverte est enregistrée', w.store.get('kairos:secrets') === '["unicorn"]');
}
// 6. Ce que le source garantit.
{
  const src = fs.readFileSync(path.join(REPO, 'src/app/services/appearance/theme.service.ts'), 'utf8');
  check("l'attribut reçoit le thème peint", /dataset\['theme'\] = this\.theme\(\)/.test(src));
  check('le stockage reçoit la préférence', /setItem\(THEME_KEY, this\._preference\(\)\)/.test(src));
  check('le système est écouté en continu', /addEventListener\('change'/.test(src));
}

if (fail) {
  console.log(`${fail} échec(s)`);
  process.exit(1);
}
console.log('theme : OK');
