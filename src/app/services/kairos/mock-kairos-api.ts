import type { BridgeError, KairosApi, LotView, Snapshot, Source, StartRequest, SubtaskChoices } from './kairos-api';
import { dayOf, scenario, type ScenarioName } from './scenarios';

const secondsOf = (ms: number): number => Math.floor(ms / 1000);

/**
 * Un faux pont pour le navigateur seul (hors Tauri) et pour les tests du magasin : il part d'un
 * scénario, répond après un petit délai, et garde l'état de ses propres actions. Il n'applique que
 * le strict nécessaire des règles du cœur (un seul lot à la fois), sans les dupliquer.
 */
export class MockKairosApi implements KairosApi {
  private current: Snapshot;
  private readonly failure: BridgeError | undefined;
  private served = 0;
  private nextId = 100;
  private readonly changed = new Set<() => void>();

  constructor(
    name: ScenarioName,
    private readonly nowMs: () => number = Date.now,
    private readonly delayMs = 80,
  ) {
    const { snapshot, failure } = scenario(name, secondsOf(nowMs()));
    this.current = snapshot;
    this.failure = failure;
  }

  async snapshot(): Promise<Snapshot> {
    await this.wait();
    // Le premier relevé réussit : l'interface a une « dernière vue connue » à griser ensuite.
    if (this.failure && this.served++ > 0) {
      throw this.failure;
    }
    return { ...this.current, server_time: secondsOf(this.nowMs()) };
  }

  async start(request: StartRequest): Promise<void> {
    await this.act();
    if (this.current.status.state !== 'idle') {
      throw { code: 'already_running', message: 'Un lot est déjà en cours : le nouveau lot ne peut pas démarrer.' } satisfies BridgeError;
    }
    this.begin(request);
  }

  async switch(request: StartRequest): Promise<void> {
    await this.act();
    this.close();
    this.begin(request);
  }

  async stop(): Promise<void> {
    await this.act();
    if (this.current.status.state === 'idle') {
      throw { code: 'nothing_running', message: 'Aucun lot en cours.' } satisfies BridgeError;
    }
    this.close();
    this.update({ state: 'idle' });
  }

  async pause(): Promise<void> {
    await this.act();
    const { status } = this.current;
    if (status.state !== 'running') {
      throw { code: 'nothing_running', message: 'Aucun lot en cours.' } satisfies BridgeError;
    }
    const now = secondsOf(this.nowMs());
    this.close();
    this.update({ state: 'paused', lot: status.lot, since: now, duration: now - status.since, launched_by: status.launched_by, workdir: status.workdir });
  }

  async resume(): Promise<void> {
    await this.act();
    const { status } = this.current;
    if (status.state !== 'paused') {
      throw { code: 'nothing_to_resume', message: 'Aucune pause à reprendre.' } satisfies BridgeError;
    }
    this.open(status.lot, secondsOf(this.nowMs()), status.launched_by, status.workdir);
  }

  async subtaskChoices(_parent: string | null): Promise<SubtaskChoices> {
    await this.wait();
    return this.current.choices;
  }

  onChanged(callback: () => void): () => void {
    this.changed.add(callback);
    return () => this.changed.delete(callback);
  }

  async setPopoverHeight(_pixels: number): Promise<void> {}

  async quit(_stopLot: boolean): Promise<void> {}

  onQuitRequested(_callback: () => void): () => void {
    return () => {};
  }

  private wait(): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, this.delayMs));
  }

  /** Une écriture : refusée tant que la base est « verrouillée » ou « trop récente ». */
  private async act(): Promise<void> {
    await this.wait();
    if (this.failure && this.served > 0) {
      throw this.failure;
    }
  }

  private begin(request: StartRequest): void {
    const lot: LotView = {
      id: this.nextId++,
      parent_monday_id: request.parent_monday_id ?? null,
      parent_label: request.parent_label ?? null,
      subtask: request.subtask ?? this.current.choices.main,
      name: request.name ?? null,
    };
    this.open(lot, request.at ?? secondsOf(this.nowMs()), 'manual', null);
  }

  private open(lot: LotView, since: number, by: Source | null, workdir: string | null): void {
    const now = secondsOf(this.nowMs());
    this.current = {
      ...this.current,
      today: dayOf([...this.current.today.entries, { id: this.nextId++, lot, start: since, end: null, removed: false }], now),
    };
    this.update({ state: 'running', lot, since, duration: now - since, today_total: this.current.today.total, launched_by: by, workdir });
  }

  /** Ferme l'entrée ouverte, s'il y en a une. */
  private close(): void {
    const now = secondsOf(this.nowMs());
    const entries = this.current.today.entries.map((e) => (e.end === null ? { ...e, end: now } : e));
    this.current = { ...this.current, today: dayOf(entries, now) };
  }

  private update(status: Snapshot['status']): void {
    this.current = { ...this.current, status, server_time: secondsOf(this.nowMs()) };
    this.changed.forEach((callback) => callback());
  }
}
