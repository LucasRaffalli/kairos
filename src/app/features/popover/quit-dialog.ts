import { ChangeDetectionStrategy, Component, ElementRef, afterNextRender, input, output, viewChild } from '@angular/core';

import { fill } from '@app/lang/fill';
import { injectT } from '@app/lang/i18n.service';
import { Button } from '@app/ui';

/**
 * « Quitter Kairos ? » quand un lot tourne (maquette 230:456) : arrêter le lot et quitter, quitter
 * en gardant le chrono (le lot reste ouvert dans la base), ou annuler (aussi Échap, géré par la
 * coquille). Le focus part sur « Annuler », le choix sans conséquence.
 */
@Component({
  selector: 'app-quit-dialog',
  imports: [Button],
  template: `
    <section class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="quit-title" aria-describedby="quit-body">
      <h2 id="quit-title" class="title">{{ t()('quit.title') }}</h2>
      <p id="quit-body" class="body">{{ fill(t()('quit.body'), { lot: summary() }) }}</p>
      <div class="actions">
        <k-button variant="danger" (pressed)="stopAndQuit.emit()">{{ t()('quit.stop') }}</k-button>
        <k-button variant="ghost" (pressed)="keepAndQuit.emit()">{{ t()('quit.keep') }}</k-button>
        <k-button #cancel variant="ghost" (pressed)="cancelled.emit()">{{ t()('quit.cancel') }}</k-button>
      </div>
    </section>
  `,
  styleUrl: './quit-dialog.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class QuitDialog {
  protected readonly t = injectT();
  protected readonly fill = fill;
  /** « Refonte · 12406850311 · 0 h 42 ». */
  readonly summary = input.required<string>();
  readonly stopAndQuit = output<void>();
  readonly keepAndQuit = output<void>();
  readonly cancelled = output<void>();

  private readonly cancel = viewChild.required<string, ElementRef<HTMLElement>>('cancel', { read: ElementRef });

  constructor() {
    afterNextRender(() => this.cancel().nativeElement.querySelector('button')?.focus());
  }
}
