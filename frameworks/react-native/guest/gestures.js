'use strict';
// react-native-gesture-handler (v3): the native side, for the OneSim guest.
//
// On iOS, RNGH attaches UIGestureRecognizers to views and reports their state machine to JS. Here the guest recognizes
// gestures itself from the host's `touch` messages (one finger): every handler on the touched view's ancestor chain
// takes part; the first one to go ACTIVE cancels the others (and RN's own responder, as RNGH's root recognizer does).
// Events go where RNGH's iOS code sends them (RNGestureHandlerManager sendEvent:withActionType:):
//   legacy API (Gesture.*, *GestureHandler): JS callbacks -> device event `onGestureHandlerStateChange`, Reanimated
//     worklets -> the attached view's `onGestureHandlerEvent`/`onGestureHandlerStateChange` (Reanimated useEvent);
//   v3 detectors (RNGestureHandlerDetector, `handlerTags` prop): direct events on the detector, `...Reanimated...` when
//     the config says dispatchesReanimatedEvents;
//   RNGestureHandlerButton views: their own topButtonPressIn/PressOut/Press/LongPress events.
// Recognized: Tap, Pan, LongPress, NativeView (buttons). Others are created but never activate (diagnosed once).
// ponytail: single pointer, no waitFor/blocksHandlers relations, no touch events (onTouches*), no fling/pinch/rotation.
const S = { UNDETERMINED: 0, FAILED: 1, BEGAN: 2, CANCELLED: 3, ACTIVE: 4, END: 5 };
const RECOGNIZED = new Set(['TapGestureHandler', 'PanGestureHandler', 'LongPressGestureHandler', 'NativeViewGestureHandler']);
const DETECTOR = 'RNGestureHandlerDetector', BUTTON = 'RNGestureHandlerButton';

const hostFunctions = (o) => { for (const f of Object.values(o)) f.__hostFunction = true; return o; };

module.exports = function createGestures(rt, { node, log, unsupported }) {
  const handlers = new Map(); // handlerTag -> {tag, name, config, view?, action?, simultaneous: Set}
  let touch = null; // the touch in progress: {parts, start: {x,y,t}, last, targetTag, captured}

  const frameOf = (tag) => rt.frames[tag];
  const inside = (tag, x, y) => { const f = frameOf(tag); return !f || (x >= f[0] && y >= f[1] && x <= f[0] + f[2] && y <= f[1] + f[3]); };
  const local = (tag, p) => { const f = frameOf(tag); return f ? [p.x - f[0], p.y - f[1]] : tag === touch.targetTag ? [p.lx, p.ly] : [p.x, p.y]; };

  // ---- event payloads ----
  function data(part) {
    const h = part.h, p = touch.last, [x, y] = local(part.view, p);
    const d = { x, y, absoluteX: p.x, absoluteY: p.y, numberOfPointers: 1, pointerType: 0, pointerInside: inside(part.view, p.x, p.y) };
    if (h.name === 'PanGestureHandler') Object.assign(d, { translationX: p.x - touch.start.x, translationY: p.y - touch.start.y, velocityX: touch.vx, velocityY: touch.vy });
    if (h.name === 'LongPressGestureHandler') d.duration = p.t - touch.start.t;
    return d;
  }
  function send(part, state, oldState) {
    const h = part.h, isState = oldState !== undefined, d = data(part), base = { handlerTag: h.tag, state };
    if (isState) base.oldState = oldState;
    if (part.detector != null) { // v3 detector: direct events, handler data nested
      const det = rt.nodes.get(part.detector), re = h.config.dispatchesReanimatedEvents ? 'Reanimated' : '';
      const type = `topGestureHandler${re}${isState ? 'StateChange' : 'Event'}`, ev = { ...base, handlerData: d, target: part.detector };
      if (re) rt.worklets.onEvent(part.detector, type, ev); else if (det) rt.dispatch(det, type, ev, isState ? 2 : 8);
    } else if (h.action === 1) rt.worklets.onEvent(h.view, isState ? 'topGestureHandlerStateChange' : 'topGestureHandlerEvent', { ...d, ...base, target: h.view });
    else rt.emit('onGestureHandlerStateChange', { ...d, ...base, target: h.view }); // iOS sends updates under this name too
  }
  function go(part, state) {
    const old = part.state;
    if (old === state) return;
    part.state = state;
    send(part, state, old);
    if (state === S.ACTIVE && !part.native) activated(part);
    if (state === S.FAILED || state === S.CANCELLED || state === S.END) node.clearTimeout(part.timer);
  }
  const live = (part) => part.state === S.BEGAN || part.state === S.ACTIVE;
  const together = (a, b) => a.h && b.h && (a.h.simultaneous.has(b.h.tag) || b.h.simultaneous.has(a.h.tag));
  // A recognizer won: cancel the rest (buttons and native views included) and RN's JS responder.
  function activated(winner) {
    for (const p of touch.parts) if (p !== winner && !together(p, winner)) cancel(p);
    if (!touch.captured) {
      touch.captured = true;
      const n = rt.nodes.get(touch.targetTag), t = touch.last;
      if (n) rt.dispatch(n, 'topTouchCancel', { touches: [], changedTouches: [{ identifier: 0, target: touch.targetTag, pageX: t.x, pageY: t.y, locationX: t.lx, locationY: t.ly, timestamp: t.t }], target: touch.targetTag, identifier: 0, pageX: t.x, pageY: t.y, timestamp: t.t }, 2);
    }
  }
  function cancel(p) {
    if (p.button != null) return button(p, 'cancel');
    if (live(p)) go(p, p.state === S.ACTIVE ? S.CANCELLED : S.FAILED);
  }
  // May `part` activate now? Not while another (non-native, non-simultaneous) recognizer holds the touch.
  const free = (part) => !touch.parts.some((p) => p !== part && p.h && !p.native && p.state === S.ACTIVE && !together(p, part));
  const activate = (part) => { if (free(part)) go(part, S.ACTIVE); else go(part, S.FAILED); };

  // ---- RNGestureHandlerButton (UIControl-like) ----
  function button(p, what) {
    const n = rt.nodes.get(p.button);
    if (!n || p.done) return;
    const t = touch.last, [x, y] = local(p.button, t);
    const ev = (type) => rt.dispatch(rt.nodes.get(p.button) || n, type, { pointerInside: inside(p.button, t.x, t.y), x, y, absoluteX: t.x, absoluteY: t.y, numberOfPointers: 1, pointerType: 0, target: p.button }, 2);
    if (what === 'start') {
      ev('topButtonPressIn');
      if (n.props.hasLongPressHandler) p.timer = node.setTimeout(rt.guard(() => { if (!p.done && touch && touch.parts.includes(p)) { p.long = true; ev('topButtonLongPress'); } }), n.props.longPressDuration ?? 500);
      return;
    }
    p.done = true;
    node.clearTimeout(p.timer);
    ev('topButtonPressOut');
    if (what === 'end' && !p.long && inside(p.button, t.x, t.y)) ev('topButtonPress');
    ev('topButtonInteractionFinished');
  }

  // ---- per-kind recognition ----
  function begin(p) {
    const c = p.h.config;
    go(p, S.BEGAN);
    if (p.h.name === 'NativeViewGestureHandler') go(p, S.ACTIVE);
    else if (p.h.name === 'LongPressGestureHandler') p.timer = node.setTimeout(rt.guard(() => { if (touch && p.state === S.BEGAN) { touch.last = { ...touch.last, t: touch.last.t + (Date.now() - touch.wall) }; activate(p); } }), c.minDurationMs ?? 500);
  }
  function move(p) {
    if (!live(p)) return;
    const c = p.h.config, t = touch.last, dx = t.x - touch.start.x, dy = t.y - touch.start.y, dist = Math.hypot(dx, dy);
    switch (p.h.name) {
      case 'TapGestureHandler':
        if ((c.maxDist != null && dist > c.maxDist) || (c.maxDeltaX != null && Math.abs(dx) > c.maxDeltaX) || (c.maxDeltaY != null && Math.abs(dy) > c.maxDeltaY)
          || (c.shouldCancelWhenOutside !== false && !inside(p.view, t.x, t.y))) go(p, S.FAILED);
        break;
      case 'LongPressGestureHandler':
        if (p.state === S.BEGAN && dist > (c.maxDist ?? 10)) go(p, S.FAILED);
        else if (p.state === S.ACTIVE) send(p, S.ACTIVE);
        break;
      case 'PanGestureHandler': {
        if (p.state === S.ACTIVE) return send(p, S.ACTIVE);
        const over = (v, lo, hi) => (lo != null && v < lo) || (hi != null && v > hi);
        if (over(dx, c.failOffsetXStart, c.failOffsetXEnd) || over(dy, c.failOffsetYStart, c.failOffsetYEnd)) return go(p, S.FAILED);
        const offsets = ['activeOffsetXStart', 'activeOffsetXEnd', 'activeOffsetYStart', 'activeOffsetYEnd'].some((k) => c[k] != null);
        if (over(dx, c.activeOffsetXStart, c.activeOffsetXEnd) || over(dy, c.activeOffsetYStart, c.activeOffsetYEnd) || dist >= (c.minDist ?? (offsets ? Infinity : 10))) {
          activate(p);
          if (p.state === S.ACTIVE) send(p, S.ACTIVE);
        }
        break;
      }
      case 'NativeViewGestureHandler': break;
    }
  }
  function end(p, cancelled) {
    if (p.button != null) return button(p, cancelled ? 'cancel' : 'end');
    if (!live(p)) return;
    if (cancelled) return go(p, p.state === S.ACTIVE ? S.CANCELLED : S.FAILED);
    const c = p.h.config;
    if (p.h.name === 'TapGestureHandler' && p.state === S.BEGAN) {
      if (touch.last.t - touch.start.t > (c.maxDurationMs ?? 500)) return go(p, S.FAILED);
      activate(p);
    }
    go(p, p.state === S.ACTIVE ? S.END : S.FAILED);
  }

  // Handlers that take part in a touch on `tag`: the view's ancestor chain, deepest first.
  function participants(tag) {
    const parts = [];
    for (let t = tag, prev = tag; t != null && rt.nodes.has(t); prev = t, t = rt.parents.get(t)) {
      const n = rt.nodes.get(t);
      if (n.viewName === BUTTON && n.props.enabled !== false) parts.push({ button: t });
      for (const h of handlers.values()) if (h.view === t) parts.push({ h, view: t });
      if (n.viewName === DETECTOR) for (const ht of n.props.handlerTags || []) { const h = handlers.get(ht); if (h) parts.push({ h, view: prev, detector: t }); }
      if (t === 1) break;
    }
    return parts.filter((p) => !p.h || (p.h.config.enabled !== false && RECOGNIZED.has(p.h.name))).map((p) => Object.assign(p, { state: S.UNDETERMINED, native: p.h && p.h.name === 'NativeViewGestureHandler' }));
  }

  return {
    // Host `touch` (before RN sees it). Returns true when a gesture owns the touch (RN's responder was cancelled).
    onTouch(m) {
      const t = { x: m.x, y: m.y, lx: m.lx ?? 0, ly: m.ly ?? 0, t: m.timestamp ?? Date.now() };
      if (m.phase === 'start') {
        if (touch) touch.parts.forEach((p) => end(p, true));
        touch = { parts: participants(m.tag), start: t, last: t, targetTag: m.tag, captured: false, vx: 0, vy: 0, wall: Date.now() };
        if (!touch.parts.length) { touch = null; return false; }
        for (const p of touch.parts) { if (p.button != null) button(p, 'start'); else begin(p); }
        return touch && touch.captured;
      }
      if (!touch) return false;
      const prev = touch.last, dt = t.t - prev.t;
      if (dt > 0) { touch.vx = ((t.x - prev.x) / dt) * 1000; touch.vy = ((t.y - prev.y) / dt) * 1000; }
      touch.last = t;
      const cur = touch;
      if (m.phase === 'move') cur.parts.forEach(move);
      else { cur.parts.forEach((p) => end(p, m.phase === 'cancel')); touch = null; }
      return cur.captured;
    },
    // Methods are JSI host functions to react-native-worklets: NativeProxy.updateGestureHandlerConfig calls one from a worklet.
    module: hostFunctions({
      createGestureHandler(name, tag, config) {
        if (!RECOGNIZED.has(name)) unsupported('RNGestureHandlerModule', `${name} (never activates)`, 'react-native-gesture-handler');
        handlers.set(tag, { tag, name, config: { ...config }, simultaneous: new Set() });
      },
      attachGestureHandler(tag, view, action) { const h = handlers.get(tag); if (h) Object.assign(h, { view, action }); },
      setGestureHandlerConfig(tag, config) { const h = handlers.get(tag); if (h) h.config = { ...config }; },
      updateGestureHandlerConfig(tag, config) { const h = handlers.get(tag); if (h) Object.assign(h.config, config); },
      configureRelations(tag, rel) { const h = handlers.get(tag); if (h) h.simultaneous = new Set(rel && rel.simultaneousHandlers || []); },
      dropGestureHandler(tag) { handlers.delete(tag); },
      flushOperations() {},
      installUIRuntimeBindings: () => true,
    }),
  };
};
