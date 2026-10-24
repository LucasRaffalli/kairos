import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/**
 * Le titre d'une section, puis un filet et une aide facultative. Deux variantes :
 * - `label` (par défaut) : petites capitales grises, « REPRENDRE », « AUJOURD'HUI · 3 h 54 » ;
 * - `step` : un bloc numéroté du formulaire, « ① Tâche parent » (pastille ronde, titre en gras).
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
  host: { '[class.step]': "variant() === 'step'" },
})
export class SectionTitle {
  readonly variant = input<'label' | 'step'>('label');
  readonly step = input<number | null>(null);
  readonly title = input.required<string>();
  readonly hint = input<string | null>(null);
}
