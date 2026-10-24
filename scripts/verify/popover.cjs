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
const lib = bundle([`${L}/clock.ts`, `${L}/form.ts`, `${L}/view.ts`, `${L}/launch.ts`, `${L}/errors.ts`, `${S}/mock-kairos-api.ts`, `${S}/kairos.store.ts`]);

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

// --- 5. Libellé de lancement --------------------------------------------------
const hm = () => '13:34';
const launch = (over) => lib.describeLaunch({ source: 'manual', startedAt: 0, workdir: null, home: '/Users/aegis', hm, ...over });
check(
  'Claude Code sous le dossier personnel',
  launch({ source: 'claude-code', workdir: '/Users/aegis/Beeside/main' }) === 'Lancé par Claude Code à 13:34 · ~/Beeside/main',
);
check('dossier hors du dossier personnel', launch({ source: 'claude-code', workdir: '/tmp/a/b/c' }) === 'Lancé par Claude Code à 13:34 · …/b/c');
check('manuel sans dossier', launch({}) === 'Lancé à 13:34');
check('source absente', launch({ source: null }) === 'Lancé à 13:34');
check('Claude Desktop', launch({ source: 'claude-desktop' }) === 'Lancé par Claude Desktop à 13:34');
check('dossier = dossier personnel', launch({ workdir: '/Users/aegis' }) === 'Lancé à 13:34 · ~');
check(
  'le début est passé à hm',
  lib.describeLaunch({ source: null, startedAt: local(2026, 9, 23, 8, 7), workdir: null, home: '', hm: lib.formatTimeOfDay }) === 'Lancé à 08:07',
);

// --- 6. Champ d'une erreur ----------------------------------------------------
check('invalid_monday_id : parent', lib.errorField('invalid_monday_id') === 'parent');
check('invalid_name : parent', lib.errorField('invalid_name') === 'parent');
check('invalid_name d’une sous-tâche perso', lib.errorField('invalid_name', true) === 'subtask');
check('in_future, too_old : début', lib.errorField('in_future') === 'time' && lib.errorField('too_old') === 'time');
check('db_unavailable : bandeau', lib.errorField('db_unavailable') === null);

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
