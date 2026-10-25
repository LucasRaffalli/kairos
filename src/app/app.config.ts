import { ApplicationConfig, provideBrowserGlobalErrorListeners, provideZonelessChangeDetection } from '@angular/core';

import { KAIROS_API, type KairosApi } from '@app/services/kairos/kairos-api';
import { KairosStore } from '@app/services/kairos/kairos.store';
import { MockKairosApi } from '@app/services/kairos/mock-kairos-api';
import { isScenario } from '@app/services/kairos/scenarios';
import { TauriKairosApi } from '@app/services/kairos/tauri-kairos-api';

/** Dans Tauri, le vrai pont ; dans un navigateur seul, le faux, dont `?scenario=` choisit l'état de départ. */
function createApi(): KairosApi {
  if ('__TAURI_INTERNALS__' in window) {
    return new TauriKairosApi();
  }
  const requested = new URLSearchParams(window.location.search).get('scenario');
  return new MockKairosApi(isScenario(requested) ? requested : 'idle');
}

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    // Zoneless : l'app est 100 % signaux + OnPush, zéro rxjs, zéro async pipe.
    // Seuls les signaux notifient, pas chaque pointermove ou chaque événement Tauri.
    provideZonelessChangeDetection(),
    { provide: KAIROS_API, useFactory: createApi },
    {
      provide: KairosStore,
      useFactory: (api: KairosApi) => {
        const store = new KairosStore(api, Date.now);
        void store.reload();
        return store;
      },
      deps: [KAIROS_API],
    },
  ],
};
