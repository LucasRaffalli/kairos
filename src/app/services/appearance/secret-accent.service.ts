import { DOCUMENT, DestroyRef, Injectable, inject } from '@angular/core';

import { ThemeService } from '@app/services/appearance/theme.service';

/** Au-delà de ce silence, la séquence en cours est oubliée. */
const FORGET_MS = 1500;

/** Décode les lettres sans conserver le mot secret dans le bundle source. */
const secretLetters = (encoded: readonly number[]): string[] => encoded.map((value) => String.fromCharCode(value ^ 0x5a));

/**
 * Le code caché de Kairos. Le mot seul ne suffit pas : on peut le taper sans y
 * penser dans un champ ou un terminal. Il est donc encadré de deux motifs de
 * flèches, qu'on ne produit pas par accident. Il n'est écrit nulle part dans
 * l'interface ; pour en sortir, on choisit un autre accent dans les réglages.
 */
@Injectable({ providedIn: 'root' })
export class SecretAccentService {
  private readonly document = inject(DOCUMENT);
  private readonly theme = inject(ThemeService);

  private readonly keys = ['arrowup', 'arrowdown', 'arrowright', ...secretLetters([47, 52, 51, 57, 53, 40, 52]), 'arrowup', 'arrowdown', 'arrowleft'];
  private progress = 0;
  private timer = 0;

  constructor() {
    this.document.addEventListener('keydown', this.onKeydown);
    inject(DestroyRef).onDestroy(() => {
      this.document.removeEventListener('keydown', this.onKeydown);
      this.clearTimer();
    });
  }

  private readonly onKeydown = (event: KeyboardEvent): void => {
    // Un raccourci n'est pas une saisie : il coupe la séquence en cours.
    if (event.metaKey || event.ctrlKey || event.altKey) {
      this.reset();
      return;
    }
    const key = event.key.toLowerCase();
    // Une lettre ou une flèche, rien d'autre : les modificateurs relâchés et
    // les touches mortes ne doivent ni avancer ni casser la séquence.
    if (!(key.startsWith('arrow') || (key.length === 1 && /[a-z]/.test(key)))) {
      return;
    }
    if (key === this.keys[this.progress]) {
      this.progress++;
      if (this.progress === this.keys.length) {
        this.reset();
        this.theme.activateSecretAccent();
        return;
      }
    } else {
      // Faux pas : on repart de zéro, mais cette touche peut être le premier pas.
      this.progress = key === this.keys[0] ? 1 : 0;
    }
    if (this.progress > 0) {
      this.restartTimer();
    } else {
      this.reset();
    }
  };

  private restartTimer(): void {
    this.clearTimer();
    this.timer = this.document.defaultView?.setTimeout(() => this.reset(), FORGET_MS) ?? 0;
  }

  private clearTimer(): void {
    if (this.timer) {
      this.document.defaultView?.clearTimeout(this.timer);
      this.timer = 0;
    }
  }

  private reset(): void {
    this.progress = 0;
    this.clearTimer();
  }
}
