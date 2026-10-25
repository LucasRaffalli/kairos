import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { KairosApi, Snapshot, StartRequest, SubtaskChoices } from './kairos-api';

/** S'abonne à un événement Tauri ; l'abonnement est asynchrone, le désabonnement non. */
function subscribe(event: string, callback: () => void): () => void {
  let unlisten: (() => void) | null = null;
  let cancelled = false;
  void listen(event, callback).then((fn) => {
    if (cancelled) {
      fn();
    } else {
      unlisten = fn;
    }
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Le vrai pont : les commandes Tauri de `src-tauri/src/bridge.rs`. Les rejets sont des `{ code, message }`. */
export class TauriKairosApi implements KairosApi {
  snapshot(): Promise<Snapshot> {
    return invoke<Snapshot>('snapshot');
  }

  async start(request: StartRequest): Promise<void> {
    await invoke('start', { request });
  }

  async switch(request: StartRequest): Promise<void> {
    await invoke('switch', { request });
  }

  async stop(): Promise<void> {
    await invoke('stop');
  }

  async pause(): Promise<void> {
    await invoke('pause');
  }

  async resume(): Promise<void> {
    await invoke('resume');
  }

  subtaskChoices(parent: string | null): Promise<SubtaskChoices> {
    return invoke<SubtaskChoices>('subtask_choices', { parent });
  }

  onChanged(callback: () => void): () => void {
    return subscribe('kairos://changed', callback);
  }

  quit(stopLot: boolean): Promise<void> {
    // La commande déclare `rename_all = "snake_case"` : l'argument s'appelle `stop_lot`.
    return invoke('quit', { stop_lot: stopLot });
  }

  onQuitRequested(callback: () => void): () => void {
    return subscribe('kairos://quit-requested', callback);
  }
}
