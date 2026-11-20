// OneSim React Native guest in the browser: the page-side twin of mainThread() in frameworks/react-native/guest/host.js.
// startGuest({ workerUrl, env, onMessage, onFile }) runs the guest in a Web Worker (browser-worker.js) and returns
// { post(hostMessage), reload(), terminate(), files }. Messages are the guest wire of docs/protocol.md §H, as objects.
// The page does the worker's HTTP (__fetch) and WebSockets (__ws), keeps tree versions monotonic across reloads,
// replays `simulate` state into each new worker and holds the session's files (`files`: path -> {data|dir}) so they
// survive reloads; RN image nodes' `__simcoreImage.path` is a key of `files`. onFile(path, bytes) runs on every file write.
// env: SIMCORE_* strings (see protocol.md "Guest launch env"); SIMCORE_METRO_URL / SIMCORE_BUNDLE_URL may be relative
// to the page (Metro sends no CORS headers: serve it through a same-origin proxy, or use a prebuilt bundle).
export function startGuest({ workerUrl, env = {}, onMessage, onFile }) {
  const files = new Map(), simulated = new Map(), dataDir = '/session';
  let worker = null, stop = () => {}, generation = -1, base = 0, last = 0;

  async function pageFetch(w, m, fetching) {
    const ac = new AbortController(), post = (x, t) => w.postMessage({ id: m.id, ...x }, t || []);
    fetching.set(m.id, ac);
    try {
      const res = await fetch(m.url, { method: m.method, headers: m.headers, redirect: m.redirect || 'follow', body: m.body || undefined, signal: ac.signal });
      post({ type: '__fetchHead', status: res.status, statusText: res.statusText, url: res.url, redirected: res.redirected, headers: [...res.headers] });
      if (res.body) for (const r = res.body.getReader(); ;) { const { done, value } = await r.read(); if (done) break; post({ type: '__fetchChunk', data: value }, [value.buffer]); }
      post({ type: '__fetchEnd' });
    } catch (e) { post({ type: '__fetchError', message: String((e && e.message) || e) }); }
    finally { fetching.delete(m.id); }
  }

  function socket(w, m, sockets) {
    const emit = (event, payload) => w.postMessage({ type: '__wsEvent', event, payload });
    if (m.op === 'connect') {
      const ws = new WebSocket(m.url, m.protocols || undefined);
      ws.binaryType = 'arraybuffer';
      sockets.set(m.id, ws);
      ws.onopen = () => emit('websocketOpen', { id: m.id });
      ws.onmessage = (ev) => emit('websocketMessage', typeof ev.data === 'string' ? { id: m.id, type: 'text', data: ev.data }
        : { id: m.id, type: 'binary', data: btoa(Array.from(new Uint8Array(ev.data), (c) => String.fromCharCode(c)).join('')) });
      ws.onerror = () => emit('websocketFailed', { id: m.id, message: 'WebSocket error' });
      ws.onclose = (ev) => { sockets.delete(m.id); emit('websocketClosed', { id: m.id, code: ev.code, reason: ev.reason }); };
    } else if (m.op === 'send') sockets.get(m.id)?.send(m.data);
    else if (m.op === 'close') sockets.get(m.id)?.close(m.code, m.reason);
  }

  function start() {
    stop();
    onMessage({ type: 'hello', protocol: 1 }); // every (re)start: the renderer drops what it told the previous worker (layout)
    base = last; // tree versions stay monotonic across reloads
    const w = (worker = new Worker(workerUrl));
    const sockets = new Map(), fetching = new Map();
    stop = () => { w.terminate(); sockets.forEach((ws) => ws.close()); fetching.forEach((ac) => ac.abort()); };
    w.postMessage({ type: '__init', env, pageOrigin: location.origin, workerData: { generation: ++generation, dataDir }, files });
    for (const [module, state] of simulated) w.postMessage({ type: 'simulate', module, state, replay: true });
    w.onmessage = ({ data: m }) => {
      if (m.type === 'tree') { m.version += base; last = m.version; }
      if (w !== worker) return; // a worker that was already replaced
      if (m.type === '__fs') { if (!m.entry) return void files.delete(m.path); files.set(m.path, m.entry); return void (m.entry.data && onFile?.(m.path, m.entry.data)); }
      if (m.type === '__reload') return start();
      if (m.type === '__fetch') return void pageFetch(w, m, fetching);
      if (m.type === '__fetchAbort') return fetching.get(m.id)?.abort();
      if (m.type === '__ws') return socket(w, m, sockets);
      onMessage(m);
    };
    w.onerror = (e) => { e.preventDefault(); onMessage({ type: 'error', message: String(e.message || e) }); };
  }

  start();
  return {
    files,
    post(m) {
      if (m.type === 'shutdown') return this.terminate();
      if (m.type === 'reload') return start();
      if (m.type === 'simulate') simulated.set(m.module, { ...simulated.get(m.module), ...m.state });
      worker?.postMessage(m);
    },
    reload: () => start(),
    terminate() { stop(); worker = null; },
  };
}
