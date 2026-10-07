/** Everything a keyboard shortcut can do; mirrors `laserline_egui::Action`. */
export type Action =
  | 'togglePlay'
  | 'resetTime'
  | 'stepBack'
  | 'stepForward'
  | 'toggleFullscreen'
  | 'exitFullscreen'
  | 'toggleControlsDetached'
  | 'toggleViewDetached'
  | 'toggleControlsHidden'
  | 'toggleSlicer';

/** A shortcut for help texts. */
export interface Shortcut {
  /** What the key does. */
  action: Action;
  /** Key label, e.g. `"F11"`. */
  key: string;
  /** Short description, e.g. `"fullscreen"`. */
  label: string;
}

/** Every shortcut, in help order. */
export const SHORTCUTS: readonly Shortcut[] = [
  { action: 'toggleFullscreen', key: 'F11', label: 'fullscreen' },
  { action: 'exitFullscreen', key: 'Esc', label: 'exit fullscreen' },
  { action: 'toggleControlsDetached', key: 'D', label: 'detach/dock controls' },
  { action: 'toggleViewDetached', key: 'V', label: 'detach/dock view' },
  { action: 'toggleControlsHidden', key: 'H', label: 'hide controls' },
  { action: 'togglePlay', key: 'Space', label: 'play/pause' },
  { action: 'resetTime', key: 'R', label: 'reset time' },
  { action: 'stepBack', key: '←', label: 'previous profile' },
  { action: 'stepForward', key: '→', label: 'next profile' },
  { action: 'toggleSlicer', key: 'S', label: 'slicer' },
];

const KEYS: Record<string, Action> = {
  ' ': 'togglePlay',
  r: 'resetTime',
  ArrowLeft: 'stepBack',
  ArrowRight: 'stepForward',
  F11: 'toggleFullscreen',
  Escape: 'exitFullscreen',
  d: 'toggleControlsDetached',
  v: 'toggleViewDetached',
  h: 'toggleControlsHidden',
  s: 'toggleSlicer',
};

/**
 * The action bound to a key press; `undefined` for other keys, with Ctrl, Alt or Meta
 * held, and while typing into a form field.
 */
export function actionForKey(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'altKey' | 'metaKey' | 'target'>): Action | undefined {
  if (e.ctrlKey || e.altKey || e.metaKey) return undefined;
  const target = e.target as Element | null;
  if (target?.closest?.('input, select, textarea, [contenteditable]')) return undefined;
  return KEYS[e.key.length === 1 ? e.key.toLowerCase() : e.key];
}

/** One-line help of all shortcuts: `"F11 fullscreen · Esc exit fullscreen · …"`. */
export function shortcutHelp(): string {
  return SHORTCUTS.map((s) => `${s.key} ${s.label}`).join(' · ');
}
