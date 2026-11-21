// OneSim TypeScript SDK: JSON-RPC 2.0 over WebSocket (docs/api.md). Zero dependencies; erasable TS only.

// ---------- types ----------
export type Framework = "expo" | "react-native" | "swiftui";
export type Appearance = "light" | "dark";
export type Status = "created" | "starting" | "running" | "paused" | "stopping" | "stopped" | "failed" | "crashed";
export type Point = { x: number; y: number };
export type Rect = { x: number; y: number; width: number; height: number };
/** Where to act: a point, an accessibility label, or a UiNode id. */
export type Target = Point | { label: string } | { element: string };

export interface CreateSessionOptions {
  framework: Framework;
  platform?: "ios" | "android";
  device?: string;
  appearance?: Appearance;
  dynamicTypeSize?: string;
  /** Simulated iOS release: "27.2" (default) or "26.5". */
  osVersion?: string;
  metroUrl?: string;
  appPath?: string;
  rootView?: string;
  viewport?: { width: number; height: number };
  scale?: number;
  /** Load right away (default true here; the server's own default is false). */
  load?: boolean;
}

export interface SessionInfo {
  sessionId: string;
  framework: Framework;
  platform: string;
  device: string;
  viewport: { width: number; height: number };
  scale: number;
  appearance: Appearance;
  status: Status;
  treeVersion: number;
  metroUrl?: string;
  appPath?: string;
}

export interface A11yPoint {
  x: number;
  y: number;
  unit?: boolean;
  description?: string;
}

export interface UiAccessibility {
  hint?: string;
  identifier?: string;
  hidden?: boolean;
  suppressed?: boolean;
  traits?: string[];
  sortPriority?: number;
  heading?: number;
  inputLabels?: string[];
  children?: string;
  actions?: { name: string; action: number; category?: string }[];
  custom?: { key: string; id?: string; value?: string; importance?: string }[];
  elements?: string[];
  focused?: boolean;
  textContentType?: string;
  scrollStatus?: string;
  rotors?: { name: string; entries?: { label: string; id?: string }[] }[];
  chart?: boolean;
  ignoresInvertColors?: boolean;
  responds?: boolean;
  directTouch?: boolean;
  directTouchOptions?: string[];
  linked?: string;
  pair?: { role: string; id: string };
  largeContent?: boolean;
  largeLabel?: string;
  activation?: A11yPoint;
  adjustable?: { increment: number; decrement: number };
  scroll?: { top?: number; bottom?: number; leading?: number; trailing?: number };
  zoom?: { in?: number; out?: number };
  rotorEntry?: string;
  technologies?: string[];
  drag?: A11yPoint;
  drop?: A11yPoint;
  defaultFocus?: boolean;
}

export interface UiNode {
  id: string;
  kind: string;
  role: string;
  label?: string;
  text?: string;
  value?: string;
  frame: Rect;
  action?: string;
  focused?: boolean;
  disabled?: boolean;
  accessibility?: UiAccessibility;
  children: UiNode[];
}

export interface DeviceInfo {
  name: string;
  displayName: string;
  viewport: { width: number; height: number };
  scale: number;
  safeArea: { top: number; bottom: number };
}

export interface Capabilities {
  frameworks: string[];
  platforms: string[];
  methods: string[];
  events: string[];
  devices: string[];
  screenshotFormats: string[];
  fonts?: "apple" | "fallback";
  symbols?: string;
}

export interface ModuleInfo {
  name: string;
  package?: string;
  status: "implemented" | "simulated" | "unsupported";
  methods: string[];
  events: string[];
}

export interface RuntimeStatus {
  status: Status;
  treeVersion: number;
  errors: { message: string; source?: string }[];
}

export interface Screenshot {
  width: number;
  height: number;
  png: Uint8Array;
}

export interface ScreenshotOptions {
  width?: number;
  height?: number;
  scale?: number;
}

export interface EventDataMap {
  "runtime.ready": { treeVersion: number };
  "runtime.error": { message: string; source?: string };
  "runtime.stopped": { reason: "stopped" | "crashed" | "failed"; exitCode?: number };
  "runtime.paused": Record<string, never>;
  "runtime.resumed": Record<string, never>;
  "bundle.loaded": { url?: string };
  "bundle.reloaded": { url?: string };
  "bundle.error": { message: string };
  "refresh.applied": Record<string, never>;
  "render.updated": { treeVersion: number };
  "navigation.changed": { route: string; params?: unknown; stack?: string[] };
  "module.unsupported": { module: string; method: string; package?: string };
  "module.called": { module: string; method: string; args: unknown };
}
export type EventName = keyof EventDataMap;
export type SimEvent<N extends EventName = EventName> = {
  [K in N]: { sessionId: string; seq: number; name: K; time: number; data: EventDataMap[K] };
}[N];
export type EventHandler<N extends EventName = EventName> = (event: SimEvent<N>) => void;

export interface ClientOptions {
  /** http://, https://, ws:// or wss:// (http maps to ws). */
  url: string;
  /** Per-request timeout in ms (default 60000). */
  timeoutMs?: number;
  clientName?: string;
  clientVersion?: string;
}

// ---------- errors ----------
export class OneSimError extends Error {
  /** OneSim `error.data.code` (bad_request, not_found, ...) or a client-side code: timeout, disconnected, closed, connect_failed. */
  code: string;
  rpcCode?: number;
  sessionId?: string;
  details?: unknown;
  constructor(code: string, message: string, extra: { rpcCode?: number; sessionId?: string; details?: unknown } = {}) {
    super(message);
    this.name = "OneSimError";
    this.code = code;
    this.rpcCode = extra.rpcCode;
    this.sessionId = extra.sessionId;
    this.details = extra.details;
  }
}

// ---------- helpers ----------
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function normalizeUrl(url: string): string {
  const u = new URL(url);
  if (u.protocol === "http:") u.protocol = "ws:";
  else if (u.protocol === "https:") u.protocol = "wss:";
  else if (u.protocol !== "ws:" && u.protocol !== "wss:") throw new OneSimError("bad_request", `unsupported url scheme ${u.protocol}`);
  return u.toString();
}

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

type Pending = { resolve: (v: any) => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> };
type Listener = { name: EventName | "*"; handler: (e: SimEvent) => void };

// ---------- client ----------
export class OneSimClient {
  capabilities?: Capabilities;
  private url: string;
  private opts: ClientOptions;
  private ws: WebSocket | null = null;
  private conn: Promise<void> | null = null;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private sessions = new Map<string, Session>();
  private closed = false;
  private everConnected = false;

  constructor(opts: ClientOptions) {
    this.opts = opts;
    this.url = normalizeUrl(opts.url);
  }

  /** Open the socket and run `initialize`. Called lazily by every request; safe to call repeatedly. */
  connect(): Promise<void> {
    if (this.closed) return Promise.reject(new OneSimError("closed", "client is closed"));
    // ponytail: first connect fails fast; after a drop we retry 6x (~3 s of backoff) before giving up.
    this.conn ??= this.open(this.everConnected ? 6 : 0).catch((e) => {
      this.conn = null;
      throw e;
    });
    return this.conn;
  }

  private async open(retries: number): Promise<void> {
    for (let attempt = 0; ; attempt++) {
      try {
        const ws = await new Promise<WebSocket>((resolve, reject) => {
          const s = new WebSocket(this.url);
          s.onopen = () => resolve(s);
          s.onerror = () => reject(new OneSimError("connect_failed", `cannot connect to ${this.url}`));
        });
        this.attach(ws);
        const init = await this.rpc("initialize", {
          protocolVersion: "2.0",
          client: { name: this.opts.clientName ?? "onesim-sdk", version: this.opts.clientVersion ?? "0.2.0" },
        });
        this.capabilities = init.capabilities;
        this.everConnected = true;
        // Re-subscribe active sessions with `since` = last seen seq (replays what we missed).
        for (const s of this.sessions.values()) if (s.hasListeners) await s.syncSubscription(true).catch(() => {});
        return;
      } catch (e) {
        const ws = this.ws;
        this.ws = null;
        ws?.close();
        if (this.closed || attempt >= retries) throw e;
        await sleep(Math.min(100 * 2 ** attempt, 2000));
      }
    }
  }

  private attach(ws: WebSocket) {
    this.ws = ws;
    ws.onmessage = (m) => this.onMessage(String(m.data));
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.conn = null;
      for (const p of this.pending.values()) {
        clearTimeout(p.timer);
        p.reject(new OneSimError("disconnected", "connection lost"));
      }
      this.pending.clear();
      if (!this.closed && [...this.sessions.values()].some((s) => s.hasListeners)) this.reconnectForever();
    };
  }

  // Background reconnect for subscribers: keep trying until it works or the client is closed.
  private reconnectForever() {
    this.connect().catch(() => {
      if (!this.closed) setTimeout(() => this.reconnectForever(), 2000);
    });
  }

  private onMessage(text: string) {
    let m: any;
    try {
      m = JSON.parse(text);
    } catch {
      return;
    }
    if (m.method === "event") return void this.sessions.get(m.params?.sessionId)?.deliver(m.params);
    const p = this.pending.get(m.id);
    if (!p) return;
    this.pending.delete(m.id);
    clearTimeout(p.timer);
    if (m.error) {
      const d = m.error.data ?? {};
      p.reject(new OneSimError(d.code ?? "runtime_error", m.error.message, { rpcCode: m.error.code, sessionId: d.sessionId, details: d.details }));
    } else p.resolve(m.result);
  }

  /** Raw send on the current socket (no connect, no handshake). */
  rpc(method: string, params: unknown, timeoutMs = this.opts.timeoutMs ?? 60_000): Promise<any> {
    const ws = this.ws;
    if (!ws) return Promise.reject(new OneSimError("disconnected", "not connected"));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new OneSimError("timeout", `${method} timed out after ${timeoutMs} ms`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      try {
        ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
      } catch {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(new OneSimError("disconnected", "send failed"));
      }
    });
  }

  /** Any JSON-RPC method, connecting first if needed. */
  async request<T = any>(method: string, params?: unknown, o: { timeoutMs?: number } = {}): Promise<T> {
    await this.connect();
    if (this.ws?.readyState !== WebSocket.OPEN) {
      // Socket is closing but onclose has not fired yet: discard it and reconnect rather than send into the void.
      const dead = this.ws;
      this.ws = this.conn = null;
      dead?.close();
      await this.connect();
    }
    return this.rpc(method, params, o.timeoutMs);
  }

  async listDevices(): Promise<DeviceInfo[]> {
    return (await this.request<{ devices: DeviceInfo[] }>("device.list")).devices;
  }

  async listSessions(): Promise<SessionInfo[]> {
    return (await this.request<{ sessions: SessionInfo[] }>("session.list")).sessions;
  }

  /** Handle for an existing session id (e.g. from `listSessions`). */
  session(id: string): Session {
    let s = this.sessions.get(id);
    if (!s) this.sessions.set(id, (s = new Session(this, id)));
    return s;
  }

  async createSession(opts: CreateSessionOptions): Promise<Session> {
    const r = await this.request<{ sessionId: string }>("session.create", { load: true, ...opts });
    return this.session(r.sessionId);
  }

  /** @internal */
  forget(id: string) {
    this.sessions.delete(id);
  }

  async close(): Promise<void> {
    this.closed = true;
    const ws = this.ws;
    this.ws = null;
    this.conn = null;
    for (const p of this.pending.values()) {
      clearTimeout(p.timer);
      p.reject(new OneSimError("closed", "client closed"));
    }
    this.pending.clear();
    ws?.close();
  }

  /** @internal test hook: the live socket. */
  get _socketForTest(): WebSocket | null {
    return this.ws;
  }
}

// ---------- session ----------
export class Session {
  readonly id: string;
  private client: OneSimClient;
  private listeners = new Set<Listener>();
  private lastSeq = 0;
  private wantReplay = false;

  constructor(client: OneSimClient, id: string) {
    this.client = client;
    this.id = id;
  }

  private call<T = any>(method: string, params: object = {}): Promise<T> {
    return this.client.request<T>(method, { sessionId: this.id, ...params });
  }

  info(): Promise<SessionInfo> { return this.call("session.get"); }
  async load(o: { metroUrl?: string; appPath?: string } = {}): Promise<Status> { return (await this.call("runtime.load", o)).status; }
  async reload(): Promise<Status> { return (await this.call("runtime.reload")).status; }
  async pause(): Promise<Status> { return (await this.call("runtime.pause")).status; }
  async resume(): Promise<Status> { return (await this.call("runtime.resume")).status; }
  async stop(): Promise<Status> { return (await this.call("runtime.stop")).status; }
  status(): Promise<RuntimeStatus> { return this.call("runtime.status"); }

  /** Resolves with the hit element id (or null). */
  async tap(t: Target): Promise<string | null> { return (await this.call("input.tap", t)).hit; }
  async press(t: Target, durationMs?: number): Promise<string | null> { return (await this.call("input.press", { ...t, durationMs })).hit; }
  async type(text: string): Promise<void> { await this.call("input.type", { text }); }
  async key(key: "enter" | "backspace"): Promise<void> { await this.call("input.key", { key }); }
  /** Positive dy reveals later content. */
  async scroll(t: Target, d: { dx?: number; dy?: number }): Promise<void> { await this.call("input.scroll", { ...t, dx: d.dx ?? 0, dy: d.dy ?? 0 }); }
  async swipe(from: Point, to: Point, durationMs?: number): Promise<void> { await this.call("input.swipe", { from, to, durationMs }); }

  async screenshot(o: ScreenshotOptions = {}): Promise<Screenshot> {
    const r = await this.call("render.screenshot", { format: "png", ...o });
    return { width: r.width, height: r.height, png: base64ToBytes(r.data) };
  }
  async frame(): Promise<Screenshot & { treeVersion: number }> {
    const r = await this.call("render.frame");
    return { treeVersion: r.treeVersion, width: r.width, height: r.height, png: base64ToBytes(r.data) };
  }
  tree(): Promise<{ treeVersion: number; root: UiNode }> { return this.call("render.tree"); }
  inspect(t: { element: string } | Point): Promise<{ node: UiNode; path: string[] }> { return this.call("render.inspect", t); }

  setDevice(device: string): Promise<SessionInfo> { return this.call("device.set", { device }); }
  async setAppearance(mode: Appearance): Promise<void> { await this.call("device.appearance", { mode }); }
  async hideKeyboard(): Promise<void> { await this.call("device.keyboard", { visible: false }); }
  async modules(): Promise<ModuleInfo[]> { return (await this.call("modules.list")).modules; }
  async simulate(module: string, state: Record<string, unknown>): Promise<void> { await this.call("modules.simulate", { module, state }); }

  async destroy(): Promise<void> {
    this.listeners.clear();
    this.client.forget(this.id);
    await this.client.request("session.destroy", { sessionId: this.id });
  }

  // ----- events -----
  get hasListeners(): boolean { return this.listeners.size > 0; }

  /**
   * Listen for events ("*" = all). Returns an unsubscribe function. Live-only unless `replay` is set and
   * this is the session's first listener (then the server's buffered history is delivered first).
   */
  on<N extends EventName>(name: N | "*", handler: EventHandler<N>, o: { replay?: boolean } = {}): () => void {
    const l: Listener = { name, handler: handler as (e: SimEvent) => void };
    if (!this.hasListeners && o.replay) this.wantReplay = true;
    this.listeners.add(l);
    this.syncSubscription().catch(() => {}); // a drop is handled by reconnect; waitFor times out otherwise
    return () => {
      this.listeners.delete(l);
      if (!this.hasListeners) this.client.request("events.unsubscribe", { sessionId: this.id }).catch(() => {});
      else this.syncSubscription().catch(() => {});
    };
  }

  /** Resolve with the first matching event; register before triggering it. Rejects with `timeout`. */
  waitFor<N extends EventName>(name: N, predicate?: (e: SimEvent<N>) => boolean, timeoutMs = 30_000, replay = false): Promise<SimEvent<N>> {
    return new Promise((resolve, reject) => {
      const off = this.on(name, (e) => {
        if (predicate && !predicate(e)) return;
        clearTimeout(timer);
        off();
        resolve(e);
      }, { replay });
      const timer = setTimeout(() => {
        off();
        reject(new OneSimError("timeout", `no ${name} event within ${timeoutMs} ms`, { sessionId: this.id }));
      }, timeoutMs);
    });
  }

  /** @internal Send events.subscribe for the current listener set. `raw` = during the reconnect handshake. */
  syncSubscription(raw = false): Promise<void> {
    const names = [...this.listeners].map((l) => l.name);
    const since = this.wantReplay ? 0 : raw || this.lastSeq ? this.lastSeq : undefined;
    const params = {
      sessionId: this.id,
      ...(names.includes("*") ? {} : { names: [...new Set(names)] }),
      ...(since === undefined ? {} : { since }),
    };
    this.wantReplay = false;
    const p = raw ? this.client.rpc("events.subscribe", params) : this.client.request("events.subscribe", params);
    // Replayed events arrive before this reply, so lastSeq already covers them.
    return p.then((r: { lastSeq: number }) => { this.lastSeq = Math.max(this.lastSeq, r.lastSeq); });
  }

  /** @internal */
  deliver(e: SimEvent) {
    if (e.seq <= this.lastSeq) return; // duplicate from a replay overlap
    this.lastSeq = e.seq;
    for (const l of [...this.listeners]) {
      if (l.name !== "*" && l.name !== e.name) continue;
      try { l.handler(e); } catch { /* a throwing listener must not break the stream */ }
    }
  }
}
