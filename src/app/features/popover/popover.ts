import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { I18nService, injectT } from '@app/lang/i18n.service';

/** Le popover de la barre de menus. À l'étape 0 : l'en-tête seul, sur un écran vide. */
@Component({
  selector: 'app-popover',
  templateUrl: './popover.html',
  styleUrl: './popover.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Popover {
  private readonly i18n = inject(I18nService);
  protected readonly t = injectT();

  /** « lun. 20 oct. » : la date du jour, dans la langue de l'interface. */
  protected readonly today = computed(() =>
    new Intl.DateTimeFormat(this.i18n.language(), { weekday: 'short', day: 'numeric', month: 'short' }).format(new Date()),
  );
}
