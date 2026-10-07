import {
  ApplicationRef,
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  TemplateRef,
  afterNextRender,
  contentChild,
  inject,
  input,
} from '@angular/core';

/** Style sheets of the page (component styles are added to its head as they load). */
const SHEETS = 'style, link[rel~="stylesheet"]';

/** Keeps copies of this page's style sheets in `target`'s head; returns the stop function. */
function mirrorStyles(target: Document): () => void {
  const copies = new Map<Node, Node>();
  const add = (node: Node) => {
    if (copies.has(node) || !(node instanceof Element) || !node.matches(SHEETS)) return;
    const copy = target.importNode(node, true);
    copies.set(node, copy);
    target.head.append(copy);
  };
  document.head.querySelectorAll(SHEETS).forEach(add);
  const observer = new MutationObserver((records) => {
    for (const r of records) {
      r.addedNodes.forEach(add);
      r.removedNodes.forEach((node) => {
        (copies.get(node) as ChildNode | undefined)?.remove();
        copies.delete(node);
      });
    }
  });
  observer.observe(document.head, { childList: true });
  return () => observer.disconnect();
}

/**
 * Renders its `<ng-template>` content into another window of this page (see
 * `openPopup()` of `@javerik/laserline`), with this page's styles. The content keeps
 * its injector, so it shares the `LaserlineStore`; laserline views render into
 * windows like that, too.
 *
 * ```html
 * @if (store.slicerWindow(); as win) {
 *   <ll-window-portal [window]="win"><ng-template><ll-slicer /></ng-template></ll-window-portal>
 * }
 * ```
 */
@Component({
  selector: 'll-window-portal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: '',
  styles: ':host { display: none; }',
})
export class LlWindowPortal {
  /** The window to render into; it must not change. */
  readonly window = input.required<Window>();
  private readonly template = contentChild.required(TemplateRef);

  constructor() {
    const appRef = inject(ApplicationRef);
    const destroyRef = inject(DestroyRef);
    let cleanup = () => {};
    destroyRef.onDestroy(() => cleanup());
    // After the first render, so the content's own render hooks run in the window.
    afterNextRender(() => {
      const win = this.window();
      if (win.closed) return;
      const stopStyles = mirrorStyles(win.document);
      const view = this.template().createEmbeddedView(undefined);
      const nodes: ChildNode[] = view.rootNodes;
      win.document.body.append(...nodes);
      appRef.attachView(view);
      view.detectChanges();
      cleanup = () => {
        stopStyles();
        appRef.detachView(view);
        view.destroy();
        if (!win.closed) nodes.forEach((n) => n.remove());
      };
    });
  }
}
