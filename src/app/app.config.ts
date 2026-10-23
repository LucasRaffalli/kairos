import { ApplicationConfig, provideBrowserGlobalErrorListeners, provideZonelessChangeDetection } from '@angular/core';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    // Zoneless : l'app est 100 % signaux + OnPush, zéro rxjs, zéro async pipe.
    // Seuls les signaux notifient, pas chaque pointermove ou chaque événement Tauri.
    provideZonelessChangeDetection(),
  ],
};
