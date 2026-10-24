import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';

let nextId = 0;

/**
 * Un champ de saisie avec son libellé. `error` met le champ en erreur (`aria-invalid`) et affiche
 * le message dessous ; `hint` est une aide discrète. Les deux sont lus avec le champ (`aria-describedby`).
 */
@Component({
  selector: 'k-field',
  template: `
    <label class="label" [for]="inputId">{{ label() }}</label>
    <div class="box focus-ring" [class.is-invalid]="invalid()">
      <input
        [id]="inputId"
        class="input"
        [class.mono]="mono()"
        type="text"
        autocomplete="off"
        spellcheck="false"
        [placeholder]="placeholder()"
        [value]="value()"
        [attr.aria-invalid]="invalid()"
        [attr.aria-describedby]="describedBy()"
        (input)="value.set($any($event.target).value)"
      />
    </div>
    @if (error(); as message) {
      <p class="message error" [id]="errorId">{{ message }}</p>
    } @else if (hint(); as text) {
      <p class="message" [id]="hintId">{{ text }}</p>
    }
  `,
  styleUrl: './field.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Field {
  readonly label = input.required<string>();
  readonly placeholder = input('');
  /** La police à chasse fixe, pour les identifiants et les heures. */
  readonly mono = input(false);
  readonly error = input<string | null>(null);
  readonly hint = input<string | null>(null);
  readonly value = model('');

  private readonly uid = nextId++;
  protected readonly inputId = `k-field-${this.uid}`;
  protected readonly errorId = `${this.inputId}-error`;
  protected readonly hintId = `${this.inputId}-hint`;

  protected readonly invalid = computed(() => !!this.error());
  protected readonly describedBy = computed(() => (this.error() ? this.errorId : this.hint() ? this.hintId : null));
}
