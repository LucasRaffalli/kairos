// La couche Angular du chronomètre, vérifiée sur du code COMPILÉ (esbuild, comme whats-new.cjs) :
// la logique pure de `logic/`, puis le magasin d'état branché sur le faux pont, sans composant.
'use strict';
const path = require('path');

// Les heures s'écrivent en heure de Paris : on y vérifie le changement d'heure.
process.env.TZ = 'Europe/Paris';

const REPO = process.env.REPO ?? path.join(__dirname, '..', '..');
const esbuild = require(path.join(REPO, 'node_modules', 'esbuild'));

let fail = 0;
const check = (label, ok, extra) => {
  if (!ok) {
    fail++;
    console.log(`ÉCHEC  ${label}${extra !== undefined ? ' :: ' + extra : ''}`);
  }
};

/** Compile en un seul module les fichiers demandés (alias @app/* du tsconfig) et rend ses exports. */
function bundle(entries) {
  const contents = entries.map((file, i) => `export * as m${i} from './${file}';`).join('\n');
  const { outputFiles } = esbuild.buildSync({
    stdin: { contents, resolveDir: REPO, loader: 'ts' },
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    target: 'es2022',
    alias: { '@app': path.join(REPO, 'src', 'app') },
    tsconfigRaw: { compilerOptions: { experimentalDecorators: true } },
    logLevel: 'silent',
  });
  const module = { exports: {} };
  new Function('require', 'module', 'exports', outputFiles[0].text)(require, module, module.exports);
  return Object.assign({}, ...Object.values(module.exports));
}

const L = 'src/app/features/popover/logic';
const S = 'src/app/services/kairos';
const lib = bundle([
  `${L}/clock.ts`,
  `${L}/form.ts`,
  `${L}/view.ts`,
  `${L}/launch.ts`,
  `${L}/errors.ts`,
  `${L}/today.ts`,
  'src/app/lang/fill.ts',
  `${S}/mock-kairos-api.ts`,
  `${S}/kairos.store.ts`,
]);

// --- 1. Formats de durée ------------------------------------------------------
check('formatClock 2533', lib.formatClock(2533) === '00:42:13');
check('formatClock 0', lib.formatClock(0) === '00:00:00');
check('formatClock 86399', lib.formatClock(86399) === '23:59:59');
check('formatClock jamais négatif', lib.formatClock(-5) === '00:00:00');
check('formatHm 2520', lib.formatHm(2520) === '0 h 42');
check('formatHm 14040', lib.formatHm(14040) === '3 h 54');
check('formatHm 3900', lib.formatHm(3900) === '1 h 05');
check('formatBar 2533', lib.formatBar(2533) === '0:42');
check('formatBar 3900', lib.formatBar(3900) === '1:05');

// --- 2. « HH:MM » vers un instant local ---------------------------------------
const local = (y, mo, d, h, mi) => Math.floor(new Date(y, mo, d, h, mi).getTime() / 1000);
const noon = new Date(2026, 9, 23, 12, 0).getTime();
check('13:34 le jour même', lib.secondsFromHhmm('13:34', noon) === local(2026, 9, 23, 13, 34));
check('9:05 accepté', lib.secondsFromHhmm('9:05', noon) === local(2026, 9, 23, 9, 5));
for (const bad of ['9:5', '25:00', '12:60', 'ab', '', '13h34']) {
  check(`« ${bad} » est refusé`, lib.secondsFromHhmm(bad, noon) === null);
}
const justAfterMidnight = new Date(2026, 9, 24, 0, 5).getTime();
const late = lib.secondsFromHhmm('23:59', justAfterMidnight);
check('23:59 à 00:05 reste ce jour-là (futur : le cœur refuse)', late === local(2026, 9, 24, 23, 59) && late > justAfterMidnight / 1000);
// Fin de l'heure d'été en France : le 25 octobre 2026, 3 h redevient 2 h.
const dstDay = new Date(2026, 9, 25, 12, 0).getTime();
check('changement d’heure : 13:34 reste 13 h 34 locale', lib.secondsFromHhmm('13:34', dstDay) === local(2026, 9, 25, 13, 34));
check('changement d’heure : le jour compte 25 h', lib.secondsFromHhmm('23:00', dstDay) - lib.secondsFromHhmm('00:00', dstDay) === 24 * 3600);
check('formatTimeOfDay relit l’heure locale', lib.formatTimeOfDay(local(2026, 9, 25, 13, 34)) === '13:34');

// --- 3. Formulaire ------------------------------------------------------------
const form = (over) => ({ ...lib.emptyForm(), ...over });
const refused = lib.canSubmit(form({}));
check('ni ID ni nom', !refused.ok && refused.reason === 'need-parent-or-name');
check('un ID seul passe', lib.canSubmit(form({ parentId: '12406850311' })).ok === true);
check('un nom seul passe', lib.canSubmit(form({ parentName: 'Réunion' })).ok === true);
check('espaces seuls : refusé', lib.canSubmit(form({ parentName: '   ' })).ok === false);
const badTime = lib.canSubmit(form({ parentId: '1', time: '9:5' }));
check('début mal formé', !badTime.ok && badTime.reason === 'bad-time');
check('début vide : maintenant', lib.canSubmit(form({ parentId: '1', time: '' })).ok === true);
const req = lib.toStartRequest(form({ parentId: '12', parentName: 'Refonte', subtask: 'Back', subtaskId: ' 42 ', time: '13:34' }), noon);
check(
  'requête avec ID : le nom devient le libellé',
  req.parent_monday_id === '12' &&
    req.parent_label === 'Refonte' &&
    req.name === null &&
    req.subtask === 'Back' &&
    req.subtask_monday_id === '42' &&
    req.at === local(2026, 9, 23, 13, 34),
);
const reqName = lib.toStartRequest(form({ parentName: 'Réunion' }), noon);
check(
  'requête sans ID : le nom reste un nom',
  reqName.parent_monday_id === null && reqName.parent_label === null && reqName.name === 'Réunion' && reqName.at === null,
);

// --- 4. Choix de la vue et couleur --------------------------------------------
const lot = { id: 1, parent_monday_id: '12', parent_label: null, subtask: 'Front', name: null };
const running = { state: 'running', lot, since: 0, duration: 0, today_total: 0, launched_by: null, workdir: null };
const paused = { state: 'paused', lot, since: 0, duration: 60, launched_by: null, workdir: null };
check('idle : start', lib.pickView({ state: 'idle' }, false) === 'start');
check('running : running', lib.pickView(running, false) === 'running');
check('running + changer : switch', lib.pickView(running, true) === 'switch');
check('paused : paused', lib.pickView(paused, false) === 'paused');
check('idle ignore « changer »', lib.pickView({ state: 'idle' }, true) === 'start');
check('parentColorIndex déterministe', lib.parentColorIndex('12406850311') === lib.parentColorIndex('12406850311'));
let inRange = true;
for (const key of ['', '1', 'Réunion', '12406850311', 'x'.repeat(500), '😀']) {
  const i = lib.parentColorIndex(key);
  inRange &&= Number.isInteger(i) && i >= 0 && i <= 5;
}
check('parentColorIndex dans 0..5', inRange);

// --- 5. Libellé de lancement : des morceaux, que la vue traduit -------------------
const hm = () => '13:34';
const launch = (over) => lib.describeLaunch({ source: 'manual', startedAt: 0, workdir: null, home: '/Users/aegis', hm, ...over });
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
check(
  'Claude Code sous le dossier personnel',
  same(launch({ source: 'claude-code', workdir: '/Users/aegis/Beeside/main' }), { who: 'claude-code', time: '13:34', folder: '~/Beeside/main' }),
  JSON.stringify(launch({ source: 'claude-code', workdir: '/Users/aegis/Beeside/main' })),
);
check('dossier hors du dossier personnel', launch({ source: 'claude-code', workdir: '/tmp/a/b/c' }).folder === '…/b/c');
check('manuel sans dossier', same(launch({}), { who: null, time: '13:34', folder: null }));
check('source absente : lancé à la main', launch({ source: null }).who === null && launch({ source: undefined }).who === null);
check('source inconnue : repli, jamais « undefined »', launch({ source: 'cursor' }).who === null);
check('Claude Desktop', launch({ source: 'claude-desktop' }).who === 'claude-desktop');
check('dossier = dossier personnel', launch({ workdir: '/Users/aegis' }).folder === '~');
check(
  'le début est passé à hm',
  lib.describeLaunch({ source: null, startedAt: local(2026, 9, 23, 8, 7), workdir: null, home: '', hm: lib.formatTimeOfDay }).time === '08:07',
);
{
  // Les commentaires peuvent citer le libellé ; le code, non.
  const code = require('fs')
    .readFileSync(path.join(REPO, L, 'launch.ts'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\/\/.*$/gm, '');
  check('aucun texte en dur dans launch.ts', !/Lancé|par Claude/.test(code));
}

// --- 5 bis. Textes à trous, puces, récents, lots ---------------------------------
check('fill remplace les trous', lib.fill('Lancé à {time} · {folder}', { time: '13:34', folder: '~' }) === 'Lancé à 13:34 · ~');
check('fill laisse un trou inconnu', lib.fill('a {x}', {}) === 'a {x}');
check(
  'puces : le poste principal d’abord, puis le socle et les propres, sans doublon',
  same(lib.subtaskChips({ main: 'Front', base: ['Back', 'Front', 'Recettage'], own: ['Recette', 'Back'] }), ['Front', 'Back', 'Recettage', 'Recette']),
);
const L1 = { id: 1, parent_monday_id: '111', parent_label: 'Refonte', subtask: 'Front', name: null };
const L2 = { id: 2, parent_monday_id: '111', parent_label: 'Refonte', subtask: 'Back', name: null };
const L3 = { id: 3, parent_monday_id: null, parent_label: null, subtask: 'Front', name: 'Réunion' };
const L4 = { id: 4, parent_monday_id: '222', parent_label: null, subtask: 'Front', name: null };
const L5 = { id: 5, parent_monday_id: '333', parent_label: 'Factu', subtask: 'Front', name: null };
const rec = (lot, current = false) => ({ lot, last_end: 0, today_total: 0, current });
check(
  'récents : un parent par ID, sans les lots sans ID',
  same(lib.recentParents([rec(L1), rec(L2), rec(L3), rec(L4), rec(L5)], 2), [
    { id: '111', name: 'Refonte' },
    { id: '222', name: '' },
  ]),
  JSON.stringify(lib.recentParents([rec(L1), rec(L2), rec(L3), rec(L4), rec(L5)], 2)),
);
check(
  'reprendre : trois lots, sans le lot en cours',
  same(
    lib.quickResumes([rec(L1, true), rec(L2), rec(L3), rec(L4), rec(L5)], 3).map((l) => l.id),
    [2, 3, 4],
  ),
);
check('titre d’un lot : libellé, sinon nom, sinon ID', lib.lotTitle(L1) === 'Refonte' && lib.lotTitle(L3) === 'Réunion' && lib.lotTitle(L4) === '222');
const r1 = lib.lotRequest(L1);
check(
  'relancer un lot avec ID',
  r1.parent_monday_id === '111' && r1.parent_label === 'Refonte' && r1.name === null && r1.subtask === 'Front' && r1.at === null,
);
const r3 = lib.lotRequest(L3);
check('relancer un lot sans ID : par son nom', r3.parent_monday_id === null && r3.name === 'Réunion' && r3.subtask === 'Front');

// --- 5 ter. La liste du jour ------------------------------------------------------
{
  const e = (id, lot, start, end, removed = false) => ({ id, lot, start, end, removed });
  const day = { entries: [e(1, L1, 1000, 2000), e(2, L2, 2100, 2200, true), e(3, L3, 3000, null)] };
  const { rows, total } = lib.todayRows(day, 50);
  check(
    'jour : sans les entrées supprimées, la plus récente d’abord',
    same(
      rows.map((r) => r.id),
      [3, 1],
    ),
  );
  check('jour : l’entrée ouverte compte le chrono', rows[0].open === true && rows[0].seconds === 50 && rows[1].seconds === 1000);
  check('jour : total vivant', total === 1050);
}

// --- 6. Champ d'une erreur ----------------------------------------------------
check('invalid_monday_id : parent', lib.errorField('invalid_monday_id') === 'parent');
check('invalid_name : parent', lib.errorField('invalid_name') === 'parent');
check('invalid_name d’une sous-tâche perso', lib.errorField('invalid_name', true) === 'subtask');
check('in_future, too_old : début', lib.errorField('in_future') === 'time' && lib.errorField('too_old') === 'time');
check('db_unavailable : bandeau', lib.errorField('db_unavailable') === null);

// --- 6 bis. Les vues, par lecture du source -------------------------------------
{
  const fs = require('fs');
  const P = path.join(REPO, 'src', 'app', 'features', 'popover');
  const read = (file) => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '');
  const VIEWS = ['start-view', 'running-view', 'today-list', 'error-banner', 'quit-dialog'];
  for (const view of VIEWS) {
    check(`${view}.ts existe`, fs.existsSync(path.join(P, `${view}.ts`)));
  }

  // Tout le source de l'interface, pour les règles qui le traversent.
  const walk = (dir) =>
    fs.readdirSync(dir, { withFileTypes: true }).flatMap((d) => (d.isDirectory() ? walk(path.join(dir, d.name)) : [path.join(dir, d.name)]));
  const appFiles = walk(path.join(REPO, 'src', 'app')).filter((f) => /\.(ts|html)$/.test(f));
  const SHELL = path.join(P, 'popover.ts');
  for (const file of appFiles) {
    if (file === SHELL) continue;
    const text = read(file);
    check(`${path.relative(REPO, file)} ne ferme pas la fenêtre`, !/\.(hide|close)\(/.test(text));
  }
  const shell = read(SHELL);
  const closes = shell.match(/\.(hide|close)\(/g) ?? [];
  check('la coquille ne ferme qu’une fois, sur Échap', closes.length === 1 && /'Escape'/.test(shell), closes.join());
  check('la coquille : ⌘↵ et ⌘.', /metaKey/.test(shell) && /'Enter'/.test(shell) && /'\.'/.test(shell));
  check('la coquille : la hauteur suit le contenu', /ResizeObserver/.test(shell) && /setPopoverHeight/.test(shell));
  check('la coquille : dialogue Quitter sur onQuitRequested', /onQuitRequested/.test(shell));
  check('la coquille : ?scenario=switching ouvre « Changer de lot »', /'switching'/.test(shell) && /openSwitch\(\)/.test(shell));
  const shellHtml = read(path.join(P, 'popover.html'));
  check('la coquille : aria-live="polite"', /aria-live="polite"/.test(shellHtml));
  check('la coquille : vue grisée quand stale', /stale\(\)/.test(shellHtml));
  check(
    'closeSwitch a un appelant',
    appFiles.some((f) => !f.endsWith('kairos.store.ts') && /closeSwitch\(\)/.test(read(f))),
  );

  // Les clés de traduction utilisées par les vues existent en fr ET en en.
  const keysOf = (file) => new Set([...read(path.join(REPO, 'src', 'app', 'lang', file)).matchAll(/^\s*'([\w.-]+)':/gm)].map((m) => m[1]));
  const fr = keysOf('fr.ts');
  const en = keysOf('en.ts');
  const viewFiles = fs
    .readdirSync(P)
    .filter((f) => /\.(ts|html)$/.test(f))
    .map((f) => path.join(P, f));
  const used = new Set();
  for (const file of viewFiles) {
    for (const m of read(file).matchAll(/'((?:popover|start|running|today|error|quit|launch|switch|paused)\.[\w.-]+)'/g)) used.add(m[1]);
  }
  check('les vues utilisent des clés de traduction', used.size >= 20, String(used.size));
  for (const key of used) {
    check(`clé « ${key} » en fr et en en`, fr.has(key) && en.has(key));
  }

  // Rien de ce qui arrive après 2a : heures de fin, calendrier, engrenage, clôture, ⌘K.
  const texts = viewFiles.map(read).join('\n') + read(path.join(REPO, 'src', 'app', 'lang', 'fr.ts')) + read(path.join(REPO, 'src', 'app', 'lang', 'en.ts'));
  for (const [label, re] of [
    ['« Fin dans »', /Fin dans|End in/],
    ['le champ « Fin »', /'Fin'|Fin vide|Fin choisie|'End'/],
    ['le calendrier', /calendar|calendrier/i],
    ['l’engrenage', /gear|engrenage|réglages|settings/i],
    ['« Clôturer la journée »', /Clôturer|Close the day/],
    ['« ⌘K »', /⌘K|⌘⇧K/],
  ]) {
    check(`les vues n’affichent pas ${label}`, !re.test(texts));
  }

  // Styles des vues : que des tokens (px permis pour les tailles fixes).
  for (const file of fs.readdirSync(P).filter((f) => f.endsWith('.scss'))) {
    const css = read(path.join(P, file))
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/^\s*\/\/.*$/gm, '');
    check(`${file} : aucune couleur littérale`, !/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(|(?<![\w-])(black|white)(?![\w-])/.test(css));
    for (const prop of ['font-size', 'border-radius']) {
      for (const m of css.matchAll(new RegExp(`(?<![\\w-])${prop}\\s*:\\s*([^;]+);`, 'g'))) {
        const value = m[1].trim();
        check(`${file} : ${prop} vient d'un token`, /^var\(--[\w-]+\)$/.test(value) || value === 'inherit' || value === '0', value);
      }
    }
  }
}

// --- 7. Le magasin, sur le faux pont ------------------------------------------
const tick = () => new Promise((resolve) => setTimeout(resolve, 5));
const NOW0 = new Date(2026, 9, 23, 12, 0).getTime();

/** Un faux pont espionné : on compte les appels et on peut en faire échouer. */
function spied(scenarioName, now = () => NOW0) {
  const api = new lib.MockKairosApi(scenarioName, now, 1);
  const calls = [];
  const wrapped = {};
  for (const name of ['snapshot', 'start', 'switch', 'stop', 'pause', 'resume', 'subtaskChoices', 'setPopoverHeight', 'quit']) {
    wrapped[name] = (...args) => {
      calls.push(name);
      return wrapped.override?.[name] ? wrapped.override[name](...args) : api[name](...args);
    };
  }
  wrapped.onChanged = (cb) => api.onChanged(cb);
  wrapped.onQuitRequested = (cb) => api.onQuitRequested(cb);
  return { api: wrapped, calls, inner: api };
}

(async () => {
  // start() deux fois très vite : un seul appel.
  {
    const { api, calls } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.form.update((f) => ({ ...f, parentId: '12406850311' }));
    const first = store.start();
    const second = store.start();
    await Promise.all([first, second]);
    check('double start : un seul appel à l’API', calls.filter((c) => c === 'start').length === 1, calls.join());
    check('après start : la vue passe à « en cours »', store.view() === 'running');
    check(
      'start ne ferme jamais la fenêtre',
      calls.every((c) => c === 'snapshot' || c === 'start'),
      calls.join(),
    );
    check('après start : plus occupé, sans erreur, formulaire vidé', !store.busy() && store.error() === null && store.form().parentId === '');
    store.dispose();
  }

  // already_running alors que l'état affiché est déjà « en cours » : pas d'erreur.
  {
    const { api, inner } = spied('running');
    api.override = {
      start: async () => {
        throw { code: 'already_running', message: 'Un lot est déjà en cours.' };
      },
    };
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.form.update((f) => ({ ...f, parentId: '5' }));
    await store.start();
    check('already_running et déjà « en cours » : aucune erreur', store.error() === null && store.view() === 'running');
    store.dispose();
    void inner;
  }

  // already_running alors que l'état affiché était « rien en cours » (lancé ailleurs entre-temps) : idem après relecture.
  {
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '77' }); // l'autre programme lance
    store.form.update((f) => ({ ...f, parentId: '5' }));
    await store.start();
    check('lancé ailleurs entre-temps : la vue rattrape, sans erreur', store.view() === 'running' && store.error() === null);
    store.dispose();
  }

  // Un refus ordinaire s'affiche, tel que le cœur l'a écrit.
  {
    const { api } = spied('idle');
    api.override = {
      start: async () => {
        throw { code: 'in_future', message: '13:34 est dans le futur.' };
      },
    };
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.form.update((f) => ({ ...f, parentId: '5', time: '13:34' }));
    await store.start();
    const e = store.error();
    check(
      'refus du cœur affiché tel quel',
      e?.code === 'in_future' && e.message === '13:34 est dans le futur.' && store.stale() === false && store.view() === 'start',
    );
    check('le brouillon est gardé après un refus', store.form().parentId === '5' && store.form().time === '13:34');
    store.dispose();
  }

  // db_unavailable : la dernière vue reste, grisée ; reload() réussi la dégrise.
  {
    const { api } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    const before = store.snapshot();
    api.override = {
      snapshot: async () => {
        throw { code: 'db_unavailable', message: 'Base inaccessible.' };
      },
    };
    await store.reload();
    check('db_unavailable : le dernier snapshot reste affiché', store.snapshot() === before && store.view() === 'running');
    check('db_unavailable : stale vrai, erreur publiée', store.stale() === true && store.error()?.code === 'db_unavailable');
    api.override = undefined;
    await store.reload();
    check('reload() réussi : stale faux, erreur effacée', store.stale() === false && store.error() === null);
    store.dispose();
  }

  // Une écriture refusée par une base verrouillée grise aussi la vue.
  {
    const { api } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    api.override = {
      stop: async () => {
        throw { code: 'db_newer', message: 'Base trop récente.' };
      },
    };
    await store.stop();
    check('db_newer pendant une action : stale', store.stale() === true && store.view() === 'running' && store.error()?.code === 'db_newer');
    store.dispose();
  }

  // Le scénario « verrouillée » du faux pont : premier relevé bon, les suivants échouent.
  {
    const { api } = spied('error-locked');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    check('error-locked : première vue disponible', store.view() === 'running' && !store.stale());
    await store.reload();
    check('error-locked : vue grisée ensuite', store.stale() && store.view() === 'running');
    store.dispose();
  }

  // clockSeconds : corrigé de l'écart entre l'horloge du cœur et celle du navigateur.
  {
    let local = NOW0;
    const { api, inner } = spied('running', () => local + 2000); // le cœur a 2 s d'avance
    const store = new lib.KairosStore(api, () => local);
    await store.reload();
    const status = store.snapshot().status;
    const serverNow = store.snapshot().server_time;
    check('clockSeconds juste malgré 2 s d’écart', store.clockSeconds() === serverNow - status.since, `${store.clockSeconds()} vs ${serverNow - status.since}`);
    local += 5000;
    store.tick();
    check('clockSeconds avance avec le minuteur local', store.clockSeconds() === serverNow - status.since + 5);
    store.dispose();
    void inner;
  }
  {
    const { api } = spied('paused');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    check('en pause : le chrono est figé sur la durée de la dernière entrée', store.clockSeconds() === 2533);
    store.dispose();
  }
  {
    const { api } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    check('avant le premier relevé : 0 et vue « start »', store.snapshot() === null && store.clockSeconds() === 0 && store.view() === 'start');
    store.dispose();
  }

  // Une écriture extérieure (événement « changed ») rafraîchit la vue.
  {
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '9' }); // déclenche onChanged
    await tick();
    check('un changement extérieur relit l’état', store.view() === 'running');
    store.dispose();
    const before = store.snapshot();
    await inner.stop();
    await tick();
    check('dispose() coupe l’écoute', store.snapshot() === before);
  }

  // « Changer de lot ».
  {
    const { api, calls } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.openSwitch();
    check('openSwitch : vue switch', store.view() === 'switch');
    store.form.update((f) => ({ ...f, parentId: '555' }));
    await store.switchLot();
    check(
      'switchLot : appel switch, retour à « en cours »',
      calls.includes('switch') && store.view() === 'running' && store.snapshot().status.lot.parent_monday_id === '555',
    );
    store.dispose();
  }

  // Annuler « Changer de lot » rend la vue « en cours ».
  {
    const { api } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.openSwitch();
    store.closeSwitch();
    check('closeSwitch : retour à « en cours »', store.view() === 'running');
    store.dispose();
  }

  // Arrêter pendant « Changer de lot » referme le panneau : un lot lancé ensuite (ailleurs) s'affiche en cours.
  {
    const { api, inner } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.openSwitch();
    await store.stop();
    await inner.start({ parent_monday_id: '8' });
    await tick();
    check('stop referme « Changer de lot »', store.view() === 'running');
    store.dispose();
  }

  // Relancer un lot récent : start au repos, switch quand un lot tourne.
  {
    const { api, calls } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.form.update((f) => ({ ...f, parentName: 'brouillon' }));
    await store.launchRecent({ id: 9, parent_monday_id: '444', parent_label: 'X', subtask: 'Back', name: null });
    check('lot récent au repos : start', calls.includes('start') && !calls.includes('switch') && store.view() === 'running');
    check('lot récent : le brouillon reste', store.form().parentName === 'brouillon');
    await store.launchRecent({ id: 10, parent_monday_id: '555', parent_label: 'Y', subtask: 'Front', name: null });
    check('lot récent pendant un lot : switch', calls.includes('switch') && store.snapshot().status.lot.parent_monday_id === '555');
    store.dispose();
  }

  // Les puces suivent le parent saisi ; une réponse en retard n'écrase pas la dernière.
  {
    const { api } = spied('idle');
    const answers = {};
    api.override = {
      subtaskChoices: (parent) => new Promise((resolve) => (answers[parent ?? 'none'] = resolve)),
    };
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    check('puces : celles du relevé par défaut', store.choices().main === 'Front');
    const first = store.loadChoices('1');
    const second = store.loadChoices('2');
    answers['2']({ main: 'Front', base: [], own: ['Deux'] });
    await second;
    answers['1']({ main: 'Front', base: [], own: ['Un'] });
    await first;
    check('puces : la dernière demande gagne', store.choices().own[0] === 'Deux', JSON.stringify(store.choices()));
    store.dispose();
  }

  // already_running rendu silencieux : le brouillon est GARDÉ (le lot affiché n'est pas forcément le sien).
  {
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '77' });
    store.form.update((f) => ({ ...f, parentId: '5' }));
    await store.start();
    check('already_running silencieux : brouillon gardé', store.form().parentId === '5' && store.error() === null);
    store.dispose();
  }

  // pause / resume / stop
  {
    const { api } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await store.pause();
    check('pause', store.view() === 'paused');
    await store.resume();
    check('resume', store.view() === 'running');
    await store.stop();
    check('stop', store.view() === 'start');
    store.dispose();
  }

  // Un formulaire invalide n'appelle pas le cœur.
  {
    const { api, calls } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await store.start();
    check('formulaire invalide : aucun appel à start', !calls.includes('start'));
    store.dispose();
  }

  if (fail) {
    console.log(`${fail} échec(s)`);
    process.exit(1);
  }
  console.log('popover : OK');
})();
