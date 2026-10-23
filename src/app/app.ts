import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { Popover } from '@app/features/popover/popover';
import { SecretAccentService } from '@app/services/appearance/secret-accent.service';
import { ThemeService } from '@app/services/appearance/theme.service';
import { UpdaterService } from '@app/services/system/updater.service';

@Component({
  selector: 'app-root',
  imports: [Popover],
  template: '<app-popover />',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {
  // Instanciés ici pour que le thème soit posé et l'écoute du code caché active dès le démarrage.
  private readonly theme = inject(ThemeService);
  private readonly secret = inject(SecretAccentService);
  // Lance le contrôle automatique des mises à jour (5 s après le démarrage, puis toutes les 6 h).
  private readonly updater = inject(UpdaterService);
}
