import type { Envelope, Pattern } from '../wasm/laserline.js';

/** Default `BroadcastChannel` name of the remote protocol. */
export const REMOTE_CHANNEL = 'laserline-remote';
/** Protocol version; messages of another version are ignored. */
export const REMOTE_VERSION = 1;
/** A calibration session without a message for this long ends by itself (ms). */
export const REMOTE_IDLE_MS = 15_000;

/** A point in view pixels (the live view's drawing buffer, y down). */
export type ViewPoint = [number, number];

/** What a laserline window reports about itself, see {@link RemoteHost}. */
export interface RemoteStatus {
  /** Size of the live view in px; `[0, 0]` before its first frame. */
  viewPx: ViewPoint;
  /** Area the line covers over its whole motion, with the user's parameters; `null` before the first frame. */
  envelope: Envelope | null;
  /** Session of the calibration running now, if any. */
  session: string | null;
  /** The part of the view a camera sees, as last set by a controller. */
  field: ViewPoint[] | null;
}

/**
 * Messages to a laserline window. `to` addresses one window (its `host` id from a
 * status); `hello` goes to all of them.
 */
export type ToLaserline =
  | { v: 1; type: 'hello' }
  | { v: 1; type: 'begin'; to: string; session: string }
  | { v: 1; type: 'show'; to: string; session: string; id: number; pattern: Pattern }
  | { v: 1; type: 'end'; to: string; session: string }
  | { v: 1; type: 'camera-field'; to: string; polygon: ViewPoint[] | null; label?: string }
  | { v: 1; type: 'fit-to-field'; to: string; id: number; margin: number };

/** Messages from a laserline window; `host` identifies it. */
export type FromLaserline =
  | { v: 1; type: 'status'; host: string; status: RemoteStatus }
  | { v: 1; type: 'shown'; host: string; session: string; id: number; viewPx: ViewPoint }
  | { v: 1; type: 'fitted'; host: string; id: number; fits: boolean }
  | { v: 1; type: 'error'; host: string; session?: string; id?: number; message: string };

/** What a {@link RemoteHost} drives: the emulator and its view, e.g. `LaserlineStore`. */
export interface RemoteTarget {
  /** The current status. */
  status(): Omit<RemoteStatus, 'session'>;
  /** Keep the user's parameters aside and stop the simulation for patterns. */
  begin(): void;
  /** Draw `pattern`, then call `rendered` once it has been on screen for a frame. */
  show(pattern: Pattern, rendered: () => void): void;
  /** Put the user's parameters and playback back. */
  end(): void;
  /** Show (or with `null`, remove) the outline of the camera's field. */
  setField(polygon: ViewPoint[] | null, label: string | null): void;
  /** Fit the line into the field `margin` px inside its edges; whether it fits, `undefined` without a field. */
  fitToField(margin: number): boolean | undefined;
}

/** Options of {@link RemoteHost}. */
export interface RemoteHostOptions {
  /** Channel name, {@link REMOTE_CHANNEL} by default. */
  channel?: string;
  /** See {@link REMOTE_IDLE_MS}. */
  idleMs?: number;
}

/**
 * Lets another page of the same origin drive this laserline window over a
 * `BroadcastChannel`, e.g. a camera application calibrating against the emulated line:
 *
 * 1. `hello` → every window answers with its `status` (view size, the line's envelope).
 * 2. `begin` → the window keeps the user's parameters aside and pauses.
 * 3. `show` patterns (a blank view, lines across it) → `shown` once each is on screen.
 * 4. `end` → the user's parameters and playback come back. A session also ends when the
 *    controller goes quiet for {@link REMOTE_IDLE_MS}.
 * 5. `camera-field` → the window outlines the part of its view the camera sees, and
 *    `fit-to-field` fits the line into it.
 *
 * Only one session runs at a time; a second `begin` gets an `error`.
 */
export class RemoteHost {
  /** Identifies this window in its messages. */
  readonly id = randomId();
  private readonly channel: BroadcastChannel;
  private readonly idleMs: number;
  private session: string | null = null;
  private lastMessage = 0;
  private idleTimer?: ReturnType<typeof setInterval>;
  private lastStatus = '';
  private closed = false;

  /** Starts listening. */
  constructor(
    private readonly target: RemoteTarget,
    options: RemoteHostOptions = {},
  ) {
    this.channel = new BroadcastChannel(options.channel ?? REMOTE_CHANNEL);
    this.idleMs = options.idleMs ?? REMOTE_IDLE_MS;
    this.channel.onmessage = (e: MessageEvent) => this.receive(e.data as ToLaserline);
  }

  /** The session running now, if any. */
  get activeSession(): string | null {
    return this.session;
  }

  /** Sends the status if it changed since the last one sent, or always with `force`. */
  postStatus(force = false): void {
    if (this.closed) return;
    const status: RemoteStatus = { ...this.target.status(), session: this.session };
    const text = JSON.stringify(status);
    if (!force && text === this.lastStatus) return;
    this.lastStatus = text;
    this.post({ v: REMOTE_VERSION, type: 'status', host: this.id, status });
  }

  /** Stops listening; a running session ends. */
  close(): void {
    if (this.closed) return;
    if (this.session) this.finish();
    this.closed = true;
    this.channel.close();
  }

  private receive(msg: ToLaserline): void {
    if (this.closed || !msg || msg.v !== REMOTE_VERSION) return;
    if (msg.type === 'hello') {
      this.postStatus(true);
      return;
    }
    if (msg.to !== this.id) return;
    this.lastMessage = Date.now();
    switch (msg.type) {
      case 'begin':
        if (this.session && this.session !== msg.session) {
          this.post({ v: 1, type: 'error', host: this.id, session: msg.session, message: 'another calibration is running' });
          return;
        }
        if (!this.session) {
          this.session = msg.session;
          this.target.begin();
          this.idleTimer = setInterval(() => this.checkIdle(), Math.min(1000, this.idleMs));
        }
        this.postStatus(true);
        break;
      case 'show': {
        if (msg.session !== this.session) {
          this.post({ v: 1, type: 'error', host: this.id, session: msg.session, id: msg.id, message: 'no such session' });
          return;
        }
        const { session, id } = msg;
        this.target.show(msg.pattern, () => {
          if (this.session !== session || this.closed) return;
          this.post({ v: 1, type: 'shown', host: this.id, session, id, viewPx: this.target.status().viewPx });
        });
        break;
      }
      case 'end':
        if (msg.session === this.session) this.finish();
        break;
      case 'camera-field':
        this.target.setField(msg.polygon, msg.label ?? null);
        this.postStatus();
        break;
      case 'fit-to-field': {
        const fits = this.target.fitToField(msg.margin);
        if (fits === undefined) {
          this.post({ v: 1, type: 'error', host: this.id, id: msg.id, message: 'no camera field set' });
        } else {
          this.post({ v: 1, type: 'fitted', host: this.id, id: msg.id, fits });
          this.postStatus();
        }
        break;
      }
    }
  }

  private checkIdle(): void {
    if (this.session && Date.now() - this.lastMessage > this.idleMs) this.finish();
  }

  private finish(): void {
    clearInterval(this.idleTimer);
    this.idleTimer = undefined;
    this.session = null;
    this.target.end();
    this.postStatus(true);
  }

  private post(msg: FromLaserline): void {
    if (!this.closed) this.channel.postMessage(msg);
  }
}

function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
