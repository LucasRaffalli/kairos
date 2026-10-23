import { Injectable, computed, effect, inject, signal } from '@angular/core';

import changelogData from '../../../assets/changelog.json';
import { ChangelogEntry } from '@app/interfaces';
import { I18nService } from '@app/lang/i18n.service';
import { ChangelogTranslations, localizeChangelog, newestFirst, shouldAnnounce } from '@app/services/system/changelog-i18n';
import { UpdaterService } from '@app/services/system/updater.service';

/** La dernière version dont on a montré les nouveautés. */
const SEEN_KEY = 'kairos:seen-version';

/** Les nouveautés : présentées quand elles servent, au premier lancement d'une version pas encore annoncée. */
@Injectable({ providedIn: 'root' })
export class WhatsNewService {
  private readonly updater = inject(UpdaterService);
  private readonly i18n = inject(I18nService);

  readonly open = signal(false);
  private readonly english = signal<ChangelogTranslations | null>(null);

  /** Le journal dans la langue de l'interface, la version la plus récente en tête. */
  readonly entries = computed(() => newestFirst(localizeChangelog(changelogData as ChangelogEntry[], this.i18n.language() === 'en' ? this.english() : null)));

  constructor() {
    // L'anglais est chargé à la demande : tant que personne n'y passe, il ne pèse rien.
    effect(() => {
      if (this.i18n.language() === 'en' && !this.english()) {
        import('../../../assets/changelog.en.json').then((module) => this.english.set(module.default as ChangelogTranslations)).catch(() => undefined);
      }
    });
  }

  /** À appeler une fois la version installée connue. Ouvre les nouveautés si elles n'ont pas été vues. */
  announceIfNew(): void {
    const current = this.updater.currentVersion();
    if (current === '…' || current === '?') {
      return;
    }
    const seen = localStorage.getItem(SEEN_KEY);
    if (shouldAnnounce(current, seen)) {
      this.open.set(true);
    }
    localStorage.setItem(SEEN_KEY, current);
  }

  close(): void {
    this.open.set(false);
  }
}
