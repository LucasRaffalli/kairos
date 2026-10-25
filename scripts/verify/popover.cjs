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
  'src/app/lang/fr.ts',
  'src/app/lang/en.ts',
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

const localIso = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
// Point 1 : la date de l'en-tête se lit dans la date du cœur, pas dans l'horloge au premier rendu.
check('en-tête fr : jour et mois', /23/.test(lib.formatDayHeader('2026-10-23', 'fr')) && /oct/.test(lib.formatDayHeader('2026-10-23', 'fr')));
check('en-tête en : langue de l’interface', /Fri/.test(lib.formatDayHeader('2026-10-23', 'en')) && /Oct/.test(lib.formatDayHeader('2026-10-23', 'en')));
check('en-tête : deux jours, deux textes', lib.formatDayHeader('2026-10-23', 'fr') !== lib.formatDayHeader('2026-10-24', 'fr'));
check('en-tête : le jour est lu en date locale (pas décalé par UTC)', /sam\./.test(lib.formatDayHeader('2026-10-24', 'fr')));
check('en-tête : sans relevé, le jour de l’horloge', lib.formatDayHeader(undefined, 'fr') === lib.formatDayHeader(localIso(new Date()), 'fr'));

// --- 2. « HH:MM » vers un instant local ---------------------------------------
const local = (y, mo, d, h, mi) => Math.floor(new Date(y, mo, d, h, mi).getTime() / 1000);
const noon = new Date(2026, 9, 23, 12, 0).getTime();
check('13:34 le jour même', lib.secondsFromHhmm('13:34', noon) === local(2026, 9, 23, 13, 34));
check('9:05 accepté', lib.secondsFromHhmm('9:05', noon) === local(2026, 9, 23, 9, 5));
check('« 9:05 » entouré d’espaces accepté', lib.secondsFromHhmm('  9:05  ', noon) === local(2026, 9, 23, 9, 5));
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
check('un ID fait d’espaces : refusé', lib.canSubmit(form({ parentId: '   ' })).ok === false);
check('un ID d’espaces ne cache pas un vrai nom', lib.canSubmit(form({ parentId: '   ', parentName: 'Réunion' })).ok === true);
const reqSpaces = lib.toStartRequest(form({ parentId: '   ', parentName: 'Réunion' }), noon);
check('un ID d’espaces n’est pas envoyé au cœur', reqSpaces.parent_monday_id === null && reqSpaces.name === 'Réunion');
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
{
  const reached = new Set();
  for (let i = 0; i < 2000; i++) reached.add(lib.parentColorIndex(String(i * 7919)));
  check('parentColorIndex atteint les 6 couleurs', reached.size === 6 && [0, 1, 2, 3, 4, 5].every((i) => reached.has(i)), [...reached].join());
}

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
  'dossier personnel voisin (/Users/aegis2) : pas de ~',
  launch({ workdir: '/Users/aegis2/x/y' }).folder === '…/x/y',
  launch({ workdir: '/Users/aegis2/x/y' }).folder,
);
check('dossier avec / final sous le dossier personnel', launch({ workdir: '/Users/aegis/Beeside/main/' }).folder === '~/Beeside/main');
check(
  'dossier personnel avec / final',
  lib.shortWorkdir('/Users/aegis/Beeside/main', '/Users/aegis/') === '~/Beeside/main' && lib.shortWorkdir('/Users/aegis/', '/Users/aegis/') === '~',
);
check('dossier hors du dossier personnel avec / final', lib.shortWorkdir('/tmp/a/b/c/', '/Users/aegis') === '…/b/c');
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
const L1 = { id: 1, parent_monday_id: '111', parent_label: 'Refonte', subtask: 'Front', subtask_monday_id: '999', name: null };
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
  'relancer un lot avec ID (et l’ID de sa sous-tâche)',
  r1.parent_monday_id === '111' &&
    r1.parent_label === 'Refonte' &&
    r1.name === null &&
    r1.subtask === 'Front' &&
    r1.subtask_monday_id === '999' &&
    r1.at === null,
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

// --- 6. Champ d'une erreur : décidé d'après le brouillon -----------------------
{
  const f = (over) => ({ ...lib.emptyForm(), ...over });
  const at = (code, over) => lib.errorField(code, f(over));
  // invalid_monday_id : le cœur valide les deux ID avec le même code.
  check('ID du parent seul fautif : sous l’ID du parent', at('invalid_monday_id', { parentId: '12a', subtaskId: '42' }) === 'parent-id');
  check('ID de la sous-tâche seul fautif : sous son ID', at('invalid_monday_id', { parentId: '124', subtaskId: '4x2' }) === 'subtask-id');
  check('ID trop long (21 chiffres) : fautif', at('invalid_monday_id', { parentId: '1'.repeat(21) }) === 'parent-id');
  check('les deux ID fautifs : bandeau', at('invalid_monday_id', { parentId: 'a', subtaskId: 'b' }) === null);
  check('aucun ID fautif visible : bandeau', at('invalid_monday_id', { parentId: '12', subtaskId: '' }) === null);
  // invalid_name : une ligne, 120 caractères au plus, comme le cœur.
  check(
    'parent de 500 caractères + sous-tâche courte : sous le nom du parent',
    at('invalid_name', { parentName: 'x'.repeat(500), subtask: 'Back' }) === 'parent-name',
  );
  check('sous-tâche sur deux lignes : sous son nom', at('invalid_name', { parentName: 'Refonte', subtask: 'a\nb' }) === 'subtask-name');
  check('121 caractères (accents comptés comme un) : fautif', at('invalid_name', { subtask: 'é'.repeat(121) }) === 'subtask-name');
  check('120 caractères : pas fautif', at('invalid_name', { parentName: 'é'.repeat(120) }) === null);
  check('121 émojis (2 unités UTF-16 chacun) : fautif', at('invalid_name', { subtask: '😀'.repeat(121) }) === 'subtask-name');
  check('120 émojis : pas fautif', at('invalid_name', { parentName: '😀'.repeat(120) }) === null);
  check('les deux noms fautifs : bandeau', at('invalid_name', { parentName: 'x'.repeat(200), subtask: 'y'.repeat(200) }) === null);
  check('aucun nom fautif visible : bandeau', at('invalid_name', { parentName: 'Refonte', subtask: 'Back' }) === null);
  check('same_lot avec ID : sous l’ID', at('same_lot', { parentId: '12' }) === 'parent-id');
  check('same_lot sans ID : sous le nom', at('same_lot', { parentName: 'Réunion' }) === 'parent-name');
  check('in_future, too_old : début', at('in_future', {}) === 'time' && at('too_old', {}) === 'time');
  check('db_unavailable : bandeau', at('db_unavailable', {}) === null);
}

// --- 6 quater. Le texte d'un refus : une clé de traduction par code, le message du cœur en repli --
{
  const REQUIRED = [
    'already_running',
    'nothing_running',
    'nothing_to_resume',
    'same_lot',
    'in_future',
    'too_old',
    'clock_behind',
    'invalid_monday_id',
    'invalid_name',
    'invalid_time',
    'reference_missing',
    'db_unavailable',
    'db_newer',
    'overlap',
    'internal',
  ];
  for (const code of REQUIRED) {
    const key = lib.ERROR_KEYS[code];
    check(`refus « ${code} » : une clé`, typeof key === 'string' && key.startsWith('error.'), String(key));
    check(`refus « ${code} » : texte en fr et en en`, !!lib.fr[key] && !!lib.en[key] && lib.fr[key] !== lib.en[key], key);
  }
  for (const key of Object.values(lib.ERROR_KEYS)) {
    for (const [lang, dictionary] of [
      ['fr', lib.fr],
      ['en', lib.en],
    ]) {
      const text = dictionary[key] ?? '';
      check(`${key} (${lang}) : pas de commande à taper`, !/`|kairos |--at|--stop/.test(text), text);
    }
  }
  check(
    'db_unavailable : ne promet pas « 5 secondes »',
    !/5 (secondes|seconds)/.test(lib.fr[lib.ERROR_KEYS.db_unavailable] + lib.en[lib.ERROR_KEYS.db_unavailable]),
  );
  const tFr = (key) => lib.fr[key];
  const tEn = (key) => lib.en[key];
  const coreRefusal = { code: 'overlap', message: 'Chevauche 12 : raccourcis.' };
  check(
    'refus connu : traduit selon la langue',
    lib.errorMessage(coreRefusal, tEn) === lib.en['error.overlap'] && lib.errorMessage(coreRefusal, tFr) === lib.fr['error.overlap'],
  );
  check(
    'refus inconnu : le message du cœur en repli',
    lib.errorMessage({ code: 'day_not_open', message: 'La journée est clôturée.' }, tEn) === 'La journée est clôturée.',
  );
  check('refus « toString » : pas d’accident de prototype', lib.errorMessage({ code: 'toString', message: 'm' }, tFr) === 'm');
}

// --- 6 quinquies. Le lot affiché est-il celui qu'on a demandé ? ---------------------------
{
  const lotOf = (over) => ({ id: 1, parent_monday_id: '111', parent_label: 'Refonte', subtask: 'Front', subtask_monday_id: null, name: null, ...over });
  const asked = (over) => ({ parent_monday_id: null, parent_label: null, name: null, subtask: null, subtask_monday_id: null, at: null, ...over });
  check('même ID, sous-tâche non précisée : le même lot', lib.lotMatches(lotOf({}), asked({ parent_monday_id: '111' })));
  check('autre ID : pas le même lot', !lib.lotMatches(lotOf({}), asked({ parent_monday_id: '222' })));
  check('même ID, autre sous-tâche : pas le même lot', !lib.lotMatches(lotOf({}), asked({ parent_monday_id: '111', subtask: 'Back' })));
  check('même ID, même sous-tâche : le même lot', lib.lotMatches(lotOf({}), asked({ parent_monday_id: '111', subtask: 'Front' })));
  check(
    'autre ID de sous-tâche : pas le même lot',
    !lib.lotMatches(lotOf({ subtask_monday_id: '9' }), asked({ parent_monday_id: '111', subtask_monday_id: '8' })),
  );
  check('par le nom, même nom : le même lot', lib.lotMatches(lotOf({ parent_monday_id: null, name: 'Réunion' }), asked({ name: 'Réunion' })));
  check('par le nom, autre nom : pas le même lot', !lib.lotMatches(lotOf({ parent_monday_id: null, name: 'Réunion' }), asked({ name: 'Appel' })));
  check('un nom demandé ne correspond pas à un lot avec ID', !lib.lotMatches(lotOf({}), asked({ name: 'Réunion' })));
}

// --- 6 ter. Résumé du dialogue Quitter (maquette 230:456 : sous-tâche · ID · durée) --
{
  const running = { state: 'running', lot: L1, since: 0, duration: 0, today_total: 0, launched_by: null, workdir: null };
  check('quitSummary : sous-tâche · ID · durée', lib.quitSummary(running, 2533) === 'Front · 111 · 0 h 42', lib.quitSummary(running, 2533));
  check('quitSummary : lot sans ID ni sous-tâche → son nom', lib.quitSummary({ ...running, lot: { ...L3, subtask: null } }, 60) === 'Réunion · 0 h 01');
  check('quitSummary : rien en cours → null', lib.quitSummary({ state: 'idle' }, 0) === null);
}

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
  check('la coquille : la date vient du relevé du cœur', /formatDayHeader\(/.test(shell) && /today\.date/.test(shell) && !/new Date\(\)/.test(shell));
  check('la coquille : le bandeau traduit le refus', /errorMessage\(/.test(shell) && !/error\.message/.test(read(path.join(P, 'popover.html'))));
  check('la coquille : Réessayer suit retryable', /store\.retryable\(\)/.test(read(path.join(P, 'popover.html'))));
  check('Lancer : les erreurs de champ sont traduites', /errorMessage\(/.test(read(path.join(P, 'start-view.ts'))));
  check('la coquille : ⌘↵ et ⌘.', /metaKey/.test(shell) && /'Enter'/.test(shell) && /'\.'/.test(shell));
  check('la coquille : plus de mesure de hauteur (la fenêtre a une hauteur fixe)', !/ResizeObserver|setPopoverHeight/.test(shell));
  const shellCss = read(path.join(P, 'popover.scss'));
  check('la coquille : hauteur de la fenêtre, sans valeur en dur', /height:\s*100vh/.test(shellCss) && !/\b560px/.test(shellCss));
  check('la fenêtre fait 560 de haut, une seule source', JSON.parse(read(path.join(REPO, 'src-tauri', 'tauri.conf.json'))).app.windows[0].height === 560);
  check(
    'la liste du jour, le formulaire et « Reprendre » défilent',
    (read(path.join(P, 'today-list.ts')).match(/scroll-y/g) ?? []).length === 1 &&
      (read(path.join(P, 'start-view.html')).match(/scroll-y/g) ?? []).length === 2,
  );
  check(
    'le magasin : dialogue Quitter sur onQuitRequested',
    /onQuitRequested/.test(read(path.join(REPO, 'src', 'app', 'services', 'kairos', 'kairos.store.ts'))),
  );
  check('la coquille : ?scenario=switching ouvre « Changer de lot »', /'switching'/.test(shell) && /openSwitch\(\)/.test(shell));
  const shellHtml = read(path.join(P, 'popover.html'));
  check('la coquille : aria-live="polite"', /aria-live="polite"/.test(shellHtml));
  check('la coquille : vue grisée quand stale', /stale\(\)/.test(shellHtml));
  check(
    'closeSwitch a un appelant',
    appFiles.some((f) => !f.endsWith('kairos.store.ts') && /closeSwitch\(\)/.test(read(f))),
  );

  const startHtml = read(path.join(P, 'start-view.html'));
  check('Lancer : les blocs 1, 2, 3 en variante « étape »', (startHtml.match(/variant="step"/g) ?? []).length === 3);
  check('Lancer : l’ID de la sous-tâche reçoit son erreur', /fieldError\('subtask-id'\)/.test(startHtml));
  const running = read(path.join(P, 'running-view.ts')) + read(path.join(P, 'running-view.html'));
  check('Minuteur : « sous-tâche <ID> »', /running\.subtaskId/.test(running) && /subtask_monday_id/.test(running));
  check('la coquille lit le dialogue Quitter dans le magasin', /store\.quitting\(\)/.test(shellHtml) && !/onQuitRequested/.test(shell));
  check('Quitter : le résumé vient de quitSummary', /quitSummary/.test(shell));

  // Une classe de vue qui porte le nom d'une variante de composant (« .step ») s'applique aussi à son hôte.
  check('start-view.scss ne définit pas .step (classe de k-section-title)', !/^\.step\b/m.test(read(path.join(P, 'start-view.scss'))));

  // Écarts relevés sur les maquettes (tâche 7).
  const startCss = read(path.join(P, 'start-view.scss'));
  // 220:81 : l'aide colle au titre d'étape (3 px), les champs viennent ensuite.
  check(
    'Lancer : chaque titre d’étape et son aide forment un en-tête',
    (startHtml.match(/<div class="head">\s*<k-section-title variant="step"[^>]*\/>\s*<p class="help">/g) ?? []).length === 3,
  );
  check('Lancer : 3 px entre le titre d’étape et son aide', /\.head\s*\{[^}]*gap:\s*3px/.test(startCss));
  // 220:81 : sous les puces, un indicateur de défilement de 3 px, curseur d'accent sur un rail de bordure.
  const bar = startCss.match(/\.chips::-webkit-scrollbar\s*\{([^}]*)\}/);
  check('Lancer : rail de défilement des puces de 3 px', bar !== null && /height:\s*3px/.test(bar[1]));
  check(
    'Lancer : curseur d’accent sur un rail de bordure',
    /::-webkit-scrollbar-thumb\s*\{[^}]*background:\s*var\(--color-accent-text\)/.test(startCss) &&
      /::-webkit-scrollbar-track\s*\{[^}]*background:\s*var\(--color-border-default\)/.test(startCss),
  );
  // 223:304 : le rappel du lot courant garde son nom et « s'arrête au changement » ; Annuler va à côté du bouton.
  const current = startHtml.match(/<section class="current">([\s\S]*?)<\/section>/);
  check('Changer de lot : le rappel ne porte pas de bouton', current !== null && !/<k-button/.test(current[1]));
  check(
    'Changer de lot : Annuler à côté du bouton principal',
    /<div class="submit-row">[\s\S]*closeSwitch\(\)[\s\S]*class="submit"[\s\S]*<\/div>/.test(startHtml),
  );
  // long-names : le badge de sous-tâche ne chasse pas les ID de la ligne.
  check('Minuteur : le badge de sous-tâche est plafonné', /\.ticket\s+k-badge\s*\{[^}]*max-width:\s*50%/.test(read(path.join(P, 'running-view.scss'))));

  // Une vraie fenêtre qui reste ouverte (changement du 25 octobre) : lue dans les sources, car une
  // fenêtre macOS ne se pilote pas d'ici.
  check('la barre de glisser porte data-tauri-drag-region', /<div class="titlebar" data-tauri-drag-region><\/div>/.test(shellHtml));
  const conf = JSON.parse(read(path.join(REPO, 'src-tauri', 'tauri.conf.json'))).app.windows[0];
  check('fenêtre : boutons natifs en surimpression, titre caché', conf.decorations === true && conf.titleBarStyle === 'Overlay' && conf.hiddenTitle === true);
  check('fenêtre : opaque, ordinaire, sans Dock', conf.transparent === false && conf.alwaysOnTop === false && conf.skipTaskbar === true);
  check('fenêtre : ni réduire (pas d’icône de Dock) ni zoom', conf.minimizable === false && conf.maximizable === false && conf.resizable === false);
  const capabilities = JSON.parse(read(path.join(REPO, 'src-tauri', 'capabilities', 'default.json'))).permissions;
  check('droit de glisser la fenêtre', capabilities.includes('core:window:allow-start-dragging'));
  const rust = (file) => read(path.join(REPO, 'src-tauri', 'src', file)).split('#[cfg(test)]')[0];
  const tray = rust('tray.rs');
  check('plus de fermeture sur perte de focus', !/Focused|LastBlur|HideAfterBlur/.test(tray + rust('lib.rs')));
  const onClose = tray.match(/if let WindowEvent::CloseRequested \{[^}]*\} = event \{([\s\S]*?)\n {8}\}/);
  check('la demande de fermeture masque sans détruire', onClose !== null && /prevent_close\(\)/.test(onClose[1]) && /\.hide\(\)/.test(onClose[1]));
  const hides = fs
    .readdirSync(path.join(REPO, 'src-tauri', 'src'))
    .filter((f) => f.endsWith('.rs'))
    .reduce((n, f) => n + (rust(f).match(/\.hide\(\)/g) ?? []).length, 0);
  check('seuls ⌘W, le clic sur l’icône et la fermeture masquent la fenêtre', hides === 3, String(hides));
  check('⌘W du menu de l’app masque la fenêtre', /"cmd\+w"/.test(tray) && !/close_window\(/.test(tray));
  check('la fenêtre n’est jamais détruite', !/\.destroy\(|\.close\(\)/.test(tray));

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
  for (const name of ['snapshot', 'start', 'switch', 'stop', 'pause', 'resume', 'subtaskChoices', 'quit']) {
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
    store.form.update((f) => ({ ...f, parentId: '12406850311' })); // le lot qui tourne déjà : un double clic
    await store.start();
    check('already_running et déjà « en cours » : aucune erreur', store.error() === null && store.view() === 'running');
    store.dispose();
    void inner;
  }

  // already_running alors que l'état affiché était « rien en cours » (lancé ailleurs entre-temps) : idem après relecture,
  // SI le lot lancé est celui qu'on demandait.
  {
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '5' }); // l'autre programme lance le même lot
    store.form.update((f) => ({ ...f, parentId: '5' }));
    await store.start();
    check('même lot lancé ailleurs entre-temps : la vue rattrape, sans erreur', store.view() === 'running' && store.error() === null);
    store.dispose();
  }

  // Point 3 : un AUTRE lot lancé ailleurs (Claude lance B pendant que le formulaire demande A) : le refus se voit.
  {
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '77' }); // Claude lance B
    store.form.update((f) => ({ ...f, parentId: '5' }));
    await store.start();
    check('autre lot lancé ailleurs : B s’affiche', store.view() === 'running' && store.snapshot().status.lot.parent_monday_id === '77');
    check('autre lot lancé ailleurs : le refus est montré', store.error()?.code === 'already_running');
    check('autre lot lancé ailleurs : le brouillon est gardé', store.form().parentId === '5');
    store.dispose();
  }
  {
    // Même ID mais autre sous-tâche : ce n'est pas le lot demandé.
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '5', subtask: 'Back' });
    store.form.update((f) => ({ ...f, parentId: '5', subtask: 'Front' }));
    await store.start();
    check('même parent, autre sous-tâche : le refus est montré', store.error()?.code === 'already_running');
    store.dispose();
  }
  {
    // « Reprendre » un lot récent : le lot en cours doit avoir le même id.
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    const recent = { id: 1, parent_monday_id: '12406850311', parent_label: 'X', subtask: 'Front', subtask_monday_id: null, name: null };
    await inner.start({ parent_monday_id: '98765' });
    await store.launchRecent(recent);
    check('reprendre pendant qu’un autre lot a été lancé ailleurs : le refus est montré', store.error()?.code === 'already_running');
    store.dispose();
  }
  {
    // Le lot repris est celui que l'autre programme vient de lancer (même id) : rien à dire.
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '98765' });
    const lotId = inner.current.status.lot.id; // sans attendre : le magasin croit encore « rien en cours »
    await store.launchRecent({ id: lotId, parent_monday_id: '98765', parent_label: null, subtask: 'Front', subtask_monday_id: null, name: null });
    check('reprendre le lot déjà lancé ailleurs : aucun refus', store.error() === null && store.view() === 'running');
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

  // already_running rendu silencieux (même lot) : le brouillon est GARDÉ, il sert tel quel à « Changer de lot ».
  {
    const { api, inner } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await inner.start({ parent_monday_id: '5' });
    store.form.update((f) => ({ ...f, parentId: '5' }));
    await store.start();
    check('already_running silencieux : brouillon gardé', store.form().parentId === '5' && store.error() === null);
    store.dispose();
  }

  // Les scénarios portent l'ID de la sous-tâche (ligne « parent … · sous-tâche … »).
  {
    const { api } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    check('scénario running : ID de sous-tâche', !!store.snapshot().status.lot.subtask_monday_id);
    store.dispose();
    const c = spied('claude-launched');
    const s2 = new lib.KairosStore(c.api, () => NOW0);
    await s2.reload();
    check('scénario claude-launched : ID de sous-tâche', !!s2.snapshot().status.lot.subtask_monday_id);
    s2.dispose();
  }

  // Le lot s'arrête ailleurs pendant « Changer de lot » : le panneau se referme pour de bon.
  {
    const { api, inner } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    store.openSwitch();
    await inner.stop();
    await tick();
    check('arrêt extérieur : plus de « Changer de lot »', store.view() === 'start');
    await inner.start({ parent_monday_id: '6' });
    await tick();
    check('lot lancé ensuite : minuteur, pas le panneau', store.view() === 'running');
    store.dispose();
  }

  // Puces périmées : remises à zéro au retour à « rien en cours » et après un lancement du formulaire.
  {
    const { api, inner } = spied('running');
    api.override = { subtaskChoices: async () => ({ main: 'Front', base: ['Back'], own: ['Propre'] }) };
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await store.loadChoices('12');
    check('puces du parent demandé', store.choices().own[0] === 'Propre');
    await inner.stop();
    await tick();
    check('retour à « rien en cours » : puces propres oubliées', store.choices().own.length === 0, JSON.stringify(store.choices()));
    await store.loadChoices('12');
    store.form.update((f) => ({ ...f, parentId: '12' }));
    await store.start();
    check('après un lancement : puces propres oubliées', store.choices().own.length === 0);
    await store.loadChoices('12');
    store.resetChoices();
    check('resetChoices : puces propres oubliées', store.choices().own.length === 0);
    store.dispose();
  }

  // Dialogue Quitter : ouvert à la demande, fermé si le lot s'arrête ailleurs, erreur visible si quit() échoue.
  {
    const { api, inner, calls } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    inner.requestQuit();
    check('demande de quitter : dialogue ouvert', store.quitting() === true);
    store.cancelQuit();
    check('Annuler : dialogue fermé', store.quitting() === false);
    inner.requestQuit();
    await inner.stop();
    await tick();
    check('lot arrêté ailleurs : dialogue fermé', store.quitting() === false);
    store.dispose();

    const b = spied('running');
    const s2 = new lib.KairosStore(b.api, () => NOW0);
    await s2.reload();
    b.inner.requestQuit();
    b.api.override = {
      quit: async () => {
        throw { code: 'internal', message: 'Impossible de quitter.' };
      },
    };
    await s2.quit(true);
    check('quit() en échec : erreur au bandeau, dialogue fermé', s2.error()?.message === 'Impossible de quitter.' && s2.quitting() === false);
    b.api.override = undefined;
    b.inner.requestQuit();
    await s2.quit(false);
    check('quit(false) appelle le pont', b.calls.filter((c) => c === 'quit').length === 2);
    s2.dispose();
    void calls;
  }
  {
    // ?scenario=quit : le faux pont demande de quitter tout seul, pour voir le dialogue dans le navigateur.
    const { api } = spied('quit');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    await new Promise((resolve) => setTimeout(resolve, 30));
    check('scénario quit : dialogue ouvert', store.quitting() === true && store.view() === 'running');
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

  // M4 : l'écran était périmé (le lot s'est arrêté ailleurs) : on relit, et la vue rattrape sans bannière.
  {
    const { api, inner } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    api.override = {
      stop: async () => {
        await inner.stop(); // arrêté ailleurs juste avant
        throw { code: 'nothing_running', message: 'Aucun lot en cours.' };
      },
    };
    await store.stop();
    check('nothing_running : la vue rattrape (Lancer)', store.view() === 'start');
    check('nothing_running : pas de bannière sur l’écran Lancer', store.error() === null);
    store.dispose();
  }
  {
    const { api, inner } = spied('paused');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    api.override = {
      resume: async () => {
        await inner.stop(); // la pause a été close ailleurs
        throw { code: 'nothing_to_resume', message: 'Rien à reprendre.' };
      },
    };
    await store.resume();
    check('nothing_to_resume : la vue rattrape, sans bannière', store.view() === 'start' && store.error() === null);
    store.dispose();
  }
  {
    // Rien n'a changé côté cœur : le refus reste affiché (on ne se tait pas sans raison).
    const { api } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    api.override = {
      pause: async () => {
        throw { code: 'nothing_running', message: 'Aucun lot en cours.' };
      },
    };
    await store.pause();
    check('nothing_running sans changement d’état : la bannière reste', store.error()?.code === 'nothing_running' && store.view() === 'running');
    store.dispose();
  }
  {
    // Une relecture qui échoue garde son erreur de base : le refus périmé ne la remplace pas.
    const { api, inner } = spied('running');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    let reads = 0;
    api.override = {
      stop: async () => {
        throw { code: 'nothing_running', message: 'Aucun lot en cours.' };
      },
      snapshot: async () => {
        reads++;
        throw { code: 'db_unavailable', message: 'Base inaccessible.' };
      },
    };
    await store.stop();
    check('refus périmé + relecture en échec : l’erreur de base reste', reads > 0 && store.error()?.code === 'db_unavailable' && store.stale());
    store.dispose();
    void inner;
  }

  // M4 : « Réessayer » aussi quand le tout premier relevé échoue avec un code non périmé.
  {
    const { api } = spied('idle');
    api.override = {
      snapshot: async () => {
        throw { code: 'internal', message: 'Erreur interne.' };
      },
    };
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    check(
      'premier relevé en échec (internal) : Réessayer proposé',
      store.snapshot() === null && store.error()?.code === 'internal' && store.retryable() === true,
    );
    api.override = undefined;
    await store.reload();
    check('relevé réussi : plus de Réessayer, plus d’erreur', store.retryable() === false);
    api.override = {
      stop: async () => {
        throw { code: 'internal', message: 'Erreur interne.' };
      },
    };
    await store.stop().catch(() => {});
    store.dispose();

    const running = spied('running');
    const s2 = new lib.KairosStore(running.api, () => NOW0);
    await s2.reload();
    running.api.override = {
      stop: async () => {
        throw { code: 'internal', message: 'Erreur interne.' };
      },
    };
    await s2.stop();
    check('erreur ordinaire avec une vue connue : pas de Réessayer', s2.error()?.code === 'internal' && s2.retryable() === false);
    s2.dispose();
    const locked = spied('error-locked');
    const s3 = new lib.KairosStore(locked.api, () => NOW0);
    await s3.reload();
    await s3.reload();
    check('base inaccessible : Réessayer proposé', s3.retryable() === true);
    s3.dispose();
  }

  // Point 1 : la date de l'en-tête suit le jour du cœur, qui change à minuit.
  {
    let clock = new Date(2026, 9, 23, 23, 59).getTime();
    const { api } = spied('running', () => clock);
    const store = new lib.KairosStore(api, () => clock);
    await store.reload();
    const before = store.snapshot().today.date;
    clock += 2 * 60 * 1000; // minuit passé
    await store.reload();
    const after = store.snapshot().today.date;
    check('minuit passé : le jour du relevé change', before === '2026-10-23' && after === '2026-10-24', `${before} -> ${after}`);
    check('minuit passé : l’en-tête change', lib.formatDayHeader(before, 'fr') !== lib.formatDayHeader(after, 'fr'));
    store.dispose();
  }

  // M5 : le magasin sait pour quel parent ses puces sont en place (effacer vite un chiffre les redemande).
  {
    const { api } = spied('idle');
    const store = new lib.KairosStore(api, () => NOW0);
    await store.reload();
    check('puces : aucun parent demandé au départ', store.choicesParent === undefined);
    await store.loadChoices('1');
    check('puces : le parent demandé est retenu', store.choicesParent === '1');
    store.resetChoices();
    check('puces : réinitialisées, le parent est oublié', store.choicesParent === undefined);
    await store.loadChoices(null);
    check('puces : « aucun parent » est un parent demandé', store.choicesParent === null);
    store.dispose();
  }

  // Minuit : sans écriture en base, rien ne dit au magasin que le jour a changé ; il relit seul,
  // une seule fois par jour, et la date et le total repartent du nouveau jour.
  {
    let t = new Date(2026, 9, 23, 23, 59, 58).getTime();
    const { api, calls } = spied('running', () => t);
    const store = new lib.KairosStore(api, () => t);
    await store.reload();
    check('minuit : la date affichée est celle du jour', store.snapshot().today.date === '2026-10-23', store.snapshot().today.date);
    const before = calls.filter((c) => c === 'snapshot').length;
    t += 1000;
    store.tick();
    await tick();
    check('minuit : avant minuit, aucune relecture', calls.filter((c) => c === 'snapshot').length === before);
    t += 2000; // 00:00:01
    store.tick();
    await tick();
    check('minuit : une relecture au changement de jour', calls.filter((c) => c === 'snapshot').length === before + 1);
    check('minuit : la date change', store.snapshot().today.date === '2026-10-24', store.snapshot().today.date);
    for (let i = 0; i < 3; i++) {
      t += 1000;
      store.tick();
      await tick();
    }
    check('minuit : pas de seconde relecture', calls.filter((c) => c === 'snapshot').length === before + 1);
    store.dispose();
  }
  {
    // Le jour se juge à l'heure du cœur : avec un cœur en retard de 1,5 s sur le navigateur, la
    // relecture attend que le cœur passe minuit, puis ne part qu'une fois.
    let t = new Date(2026, 9, 23, 23, 59, 58).getTime();
    const { api, calls } = spied('running', () => t - 1500);
    const store = new lib.KairosStore(api, () => t);
    await store.reload();
    for (let i = 0; i < 6; i++) {
      t += 1000;
      store.tick();
      await tick();
    }
    check('minuit : un cœur en retard n’est relu qu’une fois, à son minuit', calls.filter((c) => c === 'snapshot').length === 2, String(calls.length));
    check('minuit : cœur en retard, la date change', store.snapshot().today.date === '2026-10-24', store.snapshot().today.date);
    store.dispose();
  }

  if (fail) {
    console.log(`${fail} échec(s)`);
    process.exit(1);
  }
  console.log('popover : OK');
})();
