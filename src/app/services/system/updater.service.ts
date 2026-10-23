import { Injectable, computed, signal } from '@angular/core';
import { getVersion } from '@tauri-apps/api/app';
import { relaunch } from '@tauri-apps/plugin-process';
import { Update, check } from '@tauri-apps/plugin-updater';

export type UpdateStatus =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'none' }
  | { kind: 'available'; version: string }
  | { kind: 'downloading'; transferred: number; total: number }
  | { kind: 'ready' }
  | { kind: 'error'; message: string };

/** Premier contrôle automatique après le lancement : on laisse l'app démarrer tranquille. */
const AUTO_CHECK_DELAY_MS = 5_000;
/** Contrôle périodique tant que l'app tourne. */
const AUTO_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

/**
 * Mises à jour signées via tauri-plugin-updater : la vérification et le
 * téléchargement se font côté Rust, la signature est contrôlée avec la clé
 * publique de tauri.conf.json avant toute installation.
 *
 * `check()` ne lève JAMAIS : hors ligne, ou si `latest.json` est absent, le
 * résultat est `{ kind: 'error' }` et l'app continue comme si de rien n'était.
 */
@Injectable({ providedIn: 'root' })
export class UpdaterService {
  private readonly _status = signal<UpdateStatus>({ kind: 'idle' });
  private readonly _currentVersion = signal('…');
  private update: Update | null = null;

  readonly status = this._status.asReadonly();
  readonly currentVersion = this._currentVersion.asReadonly();
  /** Une mise à jour attend l'utilisateur. */
  readonly updateAvailable = computed(() => ['available', 'downloading', 'ready'].includes(this._status().kind));

  constructor() {
    void getVersion()
      .then((version) => this._currentVersion.set(version))
      .catch(() => this._currentVersion.set('?'));
    // Au démarrage (différé), puis périodique. Une erreur réseau ne dérange personne.
    setTimeout(() => void this.check(true), AUTO_CHECK_DELAY_MS);
    setInterval(() => void this.check(true), AUTO_CHECK_INTERVAL_MS);
  }

  /** Vérifie s'il existe une mise à jour. En mode `silent`, une erreur ne change pas l'état affiché. */
  async check(silent = false): Promise<UpdateStatus> {
    const current = this._status().kind;
    if (current === 'checking' || current === 'downloading' || current === 'ready') {
      return this._status();
    }
    if (!silent) {
      this._status.set({ kind: 'checking' });
    }
    try {
      this.update = await check();
      this._status.set(this.update ? { kind: 'available', version: this.update.version } : { kind: 'none' });
    } catch (error) {
      if (!silent) {
        this._status.set({ kind: 'error', message: error instanceof Error ? error.message : String(error) });
      } else if (this._status().kind === 'checking') {
        this._status.set({ kind: 'idle' });
      }
      return { kind: 'error', message: error instanceof Error ? error.message : String(error) };
    }
    return this._status();
  }

  /** Télécharge, installe puis relance l'app. */
  async install(): Promise<void> {
    const update = this.update;
    if (!update) {
      return;
    }
    let transferred = 0;
    let total = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0;
        } else if (event.event === 'Progress') {
          transferred += event.data.chunkLength;
        }
        this._status.set({ kind: 'downloading', transferred, total });
      });
      this._status.set({ kind: 'ready' });
      await relaunch();
    } catch (error) {
      this._status.set({ kind: 'error', message: error instanceof Error ? error.message : String(error) });
    }
  }
}
