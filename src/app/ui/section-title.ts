import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/** Le titre d'un bloc numéroté du formulaire : « 1 · TÂCHE PARENT », un filet, puis une aide facultative. */
@Component({
  selector: 'k-section-title',
  template: `
    <h2 class="title">
      <span class="step" aria-hidden="true">{{ step() }}</span>
      {{ title() }}
    </h2>
    <span class="rule" aria-hidden="true"></span>
    @if (hint(); as text) {
      <span class="hint">{{ text }}</span>
    }
  `,
  styleUrl: './section-title.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SectionTitle {
  readonly step = input.required<number>();
  readonly title = input.required<string>();
  readonly hint = input<string | null>(null);
}
