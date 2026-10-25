import { computed, signal } from '@angular/core';

import { emptyForm, canSubmit, lotMatches, lotRequest, toStartRequest, type FormDraft } from '@app/features/popover/logic/form';
import { isStaleCode } from '@app/features/popover/logic/errors';
import { pickView, type ViewName } from '@app/features/popover/logic/view';

import { toBridgeError, type BridgeError, type KairosApi, type LotView, type Snapshot, type StartRequest, type SubtaskChoices } from './kairos-api';

/** « AAAA-MM-JJ » du jour local d'un instant (millisecondes) : heure d'été et fuseau de la machine compris. */
function localDay(ms: number): string {
  const date = new Date(ms);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

/** Les refus d'un écran en retard sur le cœur : la relecture dit si l'action est devenue sans objet. */
const OUTDATED_SCREEN = new Set(['already_running', 'nothing_running', 'nothing_to_resume']);

/** Les réglages d'une écriture : vient du formulaire (qu'on vide), et le lot qui prouverait qu'elle a déjà eu lieu. */
interface WriteOptions {
  fromForm?: boolean;
  expects?: (running: LotView) => boolean;
}

/**
 * L'état du popover : le dernier `Snapshot` du cœur, le brouillon du formulaire et l'action en cours.
 *
 * Classe ordinaire (pas de décorateur) : Angular la fournit par une fabrique dans `app.config.ts`, et
 * le script de vérification Node la teste sans compilateur Angular. Aucune règle métier ici : les
 * refus du cœur sont publiés tels quels dans `error`. Aucune action ne ferme la fenêtre.
 */
export class KairosStore {
  private readonly _snapshot = signal<Snapshot | null>(null);
  private readonly _busy = signal(false);
  private readonly _error = signal<BridgeError | null>(null);
  private readonly _stale = signal(false);
  private readonly _switching = signal(false);
  private readonly _quitting = signal(false);
  /** Les puces demandées pour le parent saisi ; `null` : celles du dernier relevé. */
  private readonly _choices = signal<SubtaskChoices | null>(null);
  /** Le parent dont les puces demandées sont en place ; `undefined` : aucune demande aboutie. */
  private askedParent: string | null | undefined;
  /** Numéro de la dernière demande de puces : une réponse plus ancienne arrivée après est ignorée. */
  private choicesRequest = 0;
  /** L'heure locale lue au dernier battement, en millisecondes. */
  private readonly nowMs = signal(0);
  /** Avance du cœur sur le navigateur à la réception du dernier relevé, en millisecondes. */
  private offsetMs = 0;
  /** Le jour local pour lequel une relecture « changement de jour » est déjà partie (une seule par jour). */
  private refreshedFor: string | null = null;
  private readonly timer: ReturnType<typeof setInterval>;
  private readonly unsubscribe: () => void;
  private readonly unsubscribeQuit: () => void;

  /** Le dernier relevé connu (gardé tel quel quand la base est inaccessible). */
  readonly snapshot = this._snapshot.asReadonly();
  /** Une action est en cours : les boutons sont inactifs. */
  readonly busy = this._busy.asReadonly();
  /** Le dernier refus du cœur, à montrer sous un champ ou dans le bandeau. */
  readonly error = this._error.asReadonly();
  /** La base est inaccessible ou trop récente : la dernière vue reste affichée, grisée. */
  readonly stale = this._stale.asReadonly();
  /** Le dialogue « Quitter » est ouvert (demande de quitter alors qu'un lot tourne). */
  readonly quitting = this._quitting.asReadonly();
  /** « Réessayer » a un sens : la base est en cause, ou il n'y a encore rien à montrer. */
  readonly retryable = computed(() => this._stale() || this._snapshot() === null);
  /** Le brouillon du formulaire « Lancer » / « Changer de lot ». */
  readonly form = signal<FormDraft>(emptyForm());

  /**
   * Les puces de sous-tâche du formulaire : celles demandées pour le parent saisi ; à défaut, le
   * poste principal et le socle du relevé, SANS ses sous-tâches propres (elles sont celles du lot
   * en cours, pas forcément du parent saisi).
   */
  readonly choices = computed<SubtaskChoices>(() => {
    const requested = this._choices();
    if (requested) {
      return requested;
    }
    const known = this._snapshot()?.choices;
    return known ? { ...known, own: [] } : { main: '', base: [], own: [] };
  });

  readonly view = computed<ViewName>(() => {
    const snapshot = this._snapshot();
    return pickView(snapshot?.status ?? { state: 'idle' }, this._switching());
  });

  /** Le chrono en secondes : figé en pause, sinon calculé depuis le début, à l'heure du cœur. */
  readonly clockSeconds = computed(() => {
    const status = this._snapshot()?.status;
    if (status?.state === 'paused') {
      return status.duration;
    }
    if (status?.state !== 'running') {
      return 0;
    }
    return Math.max(0, Math.floor((this.nowMs() + this.offsetMs) / 1000) - status.since);
  });

  constructor(
    private readonly api: KairosApi,
    private readonly now: () => number,
  ) {
    this.nowMs.set(now());
    this.timer = setInterval(() => this.tick(), 1000);
    // Une écriture venue d'ailleurs (ligne de commande, Claude) : on relit tout.
    this.unsubscribe = api.onChanged(() => void this.reload());
    this.unsubscribeQuit = api.onQuitRequested(() => this._quitting.set(true));
  }

  /**
   * Bat une seconde : relit l'horloge locale. Appelé par le minuteur interne, par la fenêtre quand
   * elle reprend le focus (et par les tests). Si le jour local a changé depuis le relevé, relit :
   * à minuit rien n'est écrit en base, donc rien d'autre ne le dirait.
   */
  tick(): void {
    this.nowMs.set(this.now());
    this.refreshOnNewDay();
  }

  /**
   * Relit le cœur quand le jour local (à l'heure du cœur) n'est plus celui du relevé. Une seule
   * fois par jour : si le cœur reste sur l'ancien jour (horloges en désaccord), on ne le harcèle pas.
   */
  private refreshOnNewDay(): void {
    const shown = this._snapshot()?.today.date;
    if (!shown) {
      return;
    }
    const day = localDay(this.nowMs() + this.offsetMs);
    if (day !== shown && day !== this.refreshedFor) {
      this.refreshedFor = day;
      void this.reload();
    }
  }

  /** Libère le minuteur et l'écoute des changements. */
  dispose(): void {
    clearInterval(this.timer);
    this.unsubscribe();
    this.unsubscribeQuit();
  }

  /** Relit l'état du cœur. En cas d'échec, la dernière vue reste (grisée si la base est en cause). */
  async reload(): Promise<void> {
    try {
      const snapshot = await this.api.snapshot();
      this.nowMs.set(this.now());
      this.offsetMs = snapshot.server_time * 1000 - this.nowMs();
      this._snapshot.set(snapshot);
      this._stale.set(false);
      if (snapshot.status.state === 'idle') {
        // Plus rien ne tourne (arrêté ici ou ailleurs) : « Changer de lot » et « Quitter » n'ont
        // plus d'objet, et les puces propres d'un ancien parent ne doivent pas réapparaître.
        this._switching.set(false);
        this._quitting.set(false);
        this.resetChoices();
      }
      if (this._error() && isStaleCode(this._error()!.code)) {
        this._error.set(null);
      }
    } catch (error) {
      this.fail(toBridgeError(error));
    }
  }

  /** Oublie les puces demandées (parent changé, ou plus rien en cours) ; une réponse en route est ignorée. */
  resetChoices(): void {
    this.choicesRequest++;
    this.askedParent = undefined;
    this._choices.set(null);
  }

  /** Redemande les puces quand le parent saisi change (`null` : aucun parent, le poste principal). */
  async loadChoices(parent: string | null): Promise<void> {
    const request = ++this.choicesRequest;
    try {
      const choices = await this.api.subtaskChoices(parent);
      if (request === this.choicesRequest) {
        this._choices.set(choices);
        this.askedParent = parent;
      }
    } catch {
      // Sans réponse, on garde les puces affichées : le cœur jugera la sous-tâche au lancement.
    }
  }

  /** Le parent dont les puces sont en place (`null` : aucun parent) ; `undefined` : à redemander. */
  get choicesParent(): string | null | undefined {
    return this.askedParent;
  }

  openSwitch(): void {
    this._switching.set(true);
  }

  closeSwitch(): void {
    this._switching.set(false);
  }

  /** Lance le lot du formulaire. Un second appel pendant le premier est ignoré. */
  start(): Promise<void> {
    const request = this.request();
    return request ? this.write(() => this.api.start(request), { fromForm: true, expects: (running) => lotMatches(running, request) }) : Promise.resolve();
  }

  /** Arrête le lot en cours et lance celui du formulaire, à la même seconde. */
  switchLot(): Promise<void> {
    const request = this.request();
    return request ? this.write(() => this.api.switch(request), { fromForm: true }) : Promise.resolve();
  }

  /**
   * « Reprendre » : relance un lot récent, maintenant. Au repos c'est un lancement, sinon un
   * changement de lot. Le brouillon du formulaire n'est pas touché.
   */
  launchRecent(lot: LotView): Promise<void> {
    const request = lotRequest(lot);
    const idle = (this._snapshot()?.status.state ?? 'idle') === 'idle';
    return this.write(() => (idle ? this.api.start(request) : this.api.switch(request)), { expects: (running) => running.id === lot.id });
  }

  cancelQuit(): void {
    this._quitting.set(false);
  }

  /**
   * Quitte l'app, en arrêtant d'abord le lot si `stopLot`. Un échec ferme le dialogue et montre
   * l'erreur dans le bandeau, plutôt que de laisser l'app ouverte sans rien dire.
   */
  async quit(stopLot: boolean): Promise<void> {
    try {
      await this.api.quit(stopLot);
    } catch (error) {
      this._quitting.set(false);
      this.fail(toBridgeError(error));
    }
  }

  stop(): Promise<void> {
    return this.write(() => this.api.stop());
  }

  pause(): Promise<void> {
    return this.write(() => this.api.pause());
  }

  resume(): Promise<void> {
    return this.write(() => this.api.resume());
  }

  /** `null` : le formulaire n'est pas lançable, le bouton reste inactif et le cœur n'est pas appelé. */
  private request(): StartRequest | null {
    return canSubmit(this.form()).ok ? toStartRequest(this.form(), this.now()) : null;
  }

  private async write(action: () => Promise<void>, { fromForm = false, expects }: WriteOptions = {}): Promise<void> {
    if (this._busy()) {
      return;
    }
    this._busy.set(true);
    this._error.set(null);
    try {
      await action();
      await this.reload();
      // Une action réussie montre son résultat : « Changer de lot » se referme toujours.
      this._switching.set(false);
      if (fromForm) {
        this.form.set(emptyForm());
        this.resetChoices();
      }
    } catch (error) {
      await this.refused(toBridgeError(error), expects);
    } finally {
      this._busy.set(false);
    }
  }

  /**
   * Un refus du cœur. Quand il vient d'un écran périmé (double clic, lot lancé, arrêté ou repris
   * ailleurs entre-temps), on relit l'état, et on se tait si la vue a rattrapé la réalité :
   * `already_running` seulement si le lot en cours est celui qu'on demandait (`expects`), sinon
   * l'utilisateur croirait son lot démarré ; les autres, si l'état a changé depuis l'écran. Le
   * brouillon est gardé : il sert tel quel à « Changer de lot ».
   */
  private async refused(error: BridgeError, expects?: (running: LotView) => boolean): Promise<void> {
    if (OUTDATED_SCREEN.has(error.code)) {
      const before = this._snapshot()?.status.state;
      await this.reload();
      // Une relecture en échec a publié sa propre erreur : on ne la remplace pas.
      if (this._error()) {
        return;
      }
      const status = this._snapshot()?.status;
      const caughtUp = error.code === 'already_running' ? status?.state === 'running' && (expects?.(status.lot) ?? true) : status?.state !== before;
      if (caughtUp) {
        return;
      }
    }
    this.fail(error);
  }

  private fail(error: BridgeError): void {
    this._error.set(error);
    if (isStaleCode(error.code)) {
      this._stale.set(true);
    }
  }
}
