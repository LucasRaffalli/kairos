import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/**
 * Le titre d'une section : « 1 · TÂCHE PARENT » pour un bloc numéroté du formulaire, ou sans numéro
 * (« REPRENDRE », « AUJOURD'HUI · 3 h 54 »), puis un filet et une aide facultative.
 */
@Component({
  selector: 'k-section-title',
  template: `
    <h2 class="title">
      @if (step(); as n) {
        <span class="step" aria-hidden="true">{{ n }}</span>
      }
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
  readonly step = input<number | null>(null);
  readonly title = input.required<string>();
  readonly hint = input<string | null>(null);
}
