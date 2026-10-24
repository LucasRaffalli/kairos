import { computed, signal } from '@angular/core';

import { emptyForm, canSubmit, toStartRequest, type FormDraft } from '@app/features/popover/logic/form';
import { isStaleCode } from '@app/features/popover/logic/errors';
import { pickView, type ViewName } from '@app/features/popover/logic/view';

import { toBridgeError, type BridgeError, type KairosApi, type Snapshot, type StartRequest } from './kairos-api';

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
  /** L'heure locale lue au dernier battement, en millisecondes. */
  private readonly nowMs = signal(0);
  /** Avance du cœur sur le navigateur à la réception du dernier relevé, en millisecondes. */
  private offsetMs = 0;
  private readonly timer: ReturnType<typeof setInterval>;
  private readonly unsubscribe: () => void;

  /** Le dernier relevé connu (gardé tel quel quand la base est inaccessible). */
  readonly snapshot = this._snapshot.asReadonly();
  /** Une action est en cours : les boutons sont inactifs. */
  readonly busy = this._busy.asReadonly();
  /** Le dernier refus du cœur, à montrer sous un champ ou dans le bandeau. */
  readonly error = this._error.asReadonly();
  /** La base est inaccessible ou trop récente : la dernière vue reste affichée, grisée. */
  readonly stale = this._stale.asReadonly();
  /** Le brouillon du formulaire « Lancer » / « Changer de lot ». */
  readonly form = signal<FormDraft>(emptyForm());

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
  }

  /** Bat une seconde : relit l'horloge locale. Appelé par le minuteur interne (et par les tests). */
  tick(): void {
    this.nowMs.set(this.now());
  }

  /** Libère le minuteur et l'écoute des changements. */
  dispose(): void {
    clearInterval(this.timer);
    this.unsubscribe();
  }

  /** Relit l'état du cœur. En cas d'échec, la dernière vue reste (grisée si la base est en cause). */
  async reload(): Promise<void> {
    try {
      const snapshot = await this.api.snapshot();
      this.tick();
      this.offsetMs = snapshot.server_time * 1000 - this.nowMs();
      this._snapshot.set(snapshot);
      this._stale.set(false);
      if (this._error() && isStaleCode(this._error()!.code)) {
        this._error.set(null);
      }
    } catch (error) {
      this.fail(toBridgeError(error));
    }
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
    return request ? this.write(() => this.api.start(request), true) : Promise.resolve();
  }

  /** Arrête le lot en cours et lance celui du formulaire, à la même seconde. */
  switchLot(): Promise<void> {
    const request = this.request();
    return request ? this.write(() => this.api.switch(request), true) : Promise.resolve();
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

  private async write(action: () => Promise<void>, fromForm = false): Promise<void> {
    if (this._busy()) {
      return;
    }
    this._busy.set(true);
    this._error.set(null);
    try {
      await action();
      await this.reload();
      if (fromForm) {
        this.form.set(emptyForm());
        this._switching.set(false);
      }
    } catch (error) {
      await this.refused(toBridgeError(error));
    } finally {
      this._busy.set(false);
    }
  }

  /**
   * Un refus du cœur. `already_running` alors que l'écran montre déjà un lot en cours (double clic,
   * ou lot lancé ailleurs entre-temps) n'est pas une erreur : on relit l'état et on se tait.
   */
  private async refused(error: BridgeError): Promise<void> {
    if (error.code === 'already_running') {
      await this.reload();
      if (this._snapshot()?.status.state === 'running') {
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
