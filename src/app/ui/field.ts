import { ChangeDetectionStrategy, Component, ElementRef, computed, input, model, output, viewChild } from '@angular/core';

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
        #control
        [id]="inputId"
        class="input"
        [class.mono]="mono()"
        type="text"
        autocomplete="off"
        spellcheck="false"
        [placeholder]="placeholder()"
        [value]="value()"
        [disabled]="disabled()"
        [attr.aria-invalid]="invalid()"
        [attr.aria-describedby]="describedBy()"
        (input)="value.set($any($event.target).value)"
        (keydown.enter)="onEnter($event)"
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
  readonly disabled = input(false);
  readonly value = model('');
  /** Émis quand on appuie sur Entrée dans le champ (pas pendant une composition IME). */
  readonly submitted = output<void>();

  private readonly control = viewChild.required<ElementRef<HTMLInputElement>>('control');

  private readonly uid = nextId++;
  protected readonly inputId = `k-field-${this.uid}`;
  protected readonly errorId = `${this.inputId}-error`;
  protected readonly hintId = `${this.inputId}-hint`;

  protected readonly invalid = computed(() => !!this.error());
  protected readonly describedBy = computed(() => (this.error() ? this.errorId : this.hint() ? this.hintId : null));

  /** Met le focus dans le champ, par exemple quand on choisit « + Autre ». */
  focus(): void {
    this.control().nativeElement.focus();
  }

  protected onEnter(event: Event): void {
    if (!(event as KeyboardEvent).isComposing) {
      this.submitted.emit();
    }
  }
}
