import { DestroyRef, Directive, ElementRef, afterNextRender, inject, input, signal } from '@angular/core';

import { moreContent } from './scroll-fade.logic';

/**
 * Un fondu au bord d'une zone qui défile, du côté où il reste du contenu : il disparaît quand on
 * arrive au bout, et quand le contenu ne déborde pas. À poser sur l'élément qui défile
 * (`overflow` auto) : `<ul kScrollFade="y">`. Le fondu est un masque (`mask-image`) : il n'a besoin
 * d'aucune couleur de fond, suit donc tous les thèmes, et ne capte aucun clic. Le style est dans
 * `styles/_base.scss` (classes `k-fade-x` et `k-fade-y`).
 */
@Directive({
  selector: '[kScrollFade]',
  host: {
    '[class.k-fade-y]': "axis() === 'y'",
    '[class.k-fade-x]': "axis() === 'x'",
    '[style.--fade-start.px]': 'start() ? fadeSize : 0',
    '[style.--fade-end.px]': 'end() ? fadeSize : 0',
    '(scroll)': 'measure()',
  },
})
export class ScrollFade {
  /** L'axe qui défile. */
  readonly axis = input<'x' | 'y'>('y', { alias: 'kScrollFade' });

  /** Taille du fondu, en pixels. */
  protected readonly fadeSize = 24;
  protected readonly start = signal(false);
  protected readonly end = signal(false);

  private readonly element = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;

  constructor() {
    afterNextRender(() => {
      this.measure();
      // La zone change de taille, ou son contenu change (liste qui s'allonge, puces ajoutées).
      const resize = new ResizeObserver(() => this.measure());
      resize.observe(this.element);
      const mutation = new MutationObserver(() => this.measure());
      mutation.observe(this.element, { childList: true, subtree: true });
      inject(DestroyRef).onDestroy(() => {
        resize.disconnect();
        mutation.disconnect();
      });
    });
  }

  protected measure(): void {
    const el = this.element;
    const vertical = this.axis() === 'y';
    const more = moreContent(
      vertical
        ? { offset: el.scrollTop, size: el.scrollHeight, client: el.clientHeight }
        : { offset: el.scrollLeft, size: el.scrollWidth, client: el.clientWidth },
    );
    this.start.set(more.before);
    this.end.set(more.after);
  }
}
