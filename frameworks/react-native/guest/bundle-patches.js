// Source patches applied to a fetched RN bundle before it runs (host.js boot, Node and browser worker alike).
// Each must hold for both Metro dev output (`(0, invariant.default)(NativeAnimatedModule, '...');`) and minified
// `expo export` output (`(0,u.default)(c,'...'),h=null,...`), so they anchor on string literals and code shape,
// never on identifier names the minifier renames. Shapes seen in RN 0.88 (tests: bundle-patches.test.mjs):
//   dev       flushQueue: () => {\n  (0, invariant.default)(NativeAnimatedModule, 'Native animated ...');\n  flushQueueImmediate = null;
//   min ios   flushQueue:()=>{if((0,u.default)(c,'Native animated ...'),h=null,0!==p.length){...
//   min and   flushQueue:b?()=>{(0,l.default)(f,'Native animated ...'),w=null,0!==V.length&&(...)}:()=>{if((0,l.default)(f,...
'use strict';

const MSG = 'Native animated module is not available';
// invariant(<module>, 'Native animated module is not available'): callee `(0, x.default)` (Babel interop) or a bare
// name (whole: the lookbehind keeps a long identifier run linear); any quote style; optional trailing comma.
const INVARIANT = String.raw`(?:\(\s*0\s*,\s*[\w$.]+\s*\)|(?<![\w$.])[\w$.]+)\(\s*(?<mod>[\w$.]+)\s*,\s*(?<q>['"\x60])${MSG}\k<q>\s*,?\s*\)`;
// Only flushQueue's invariant is followed by `flushQueueImmediate = null` (`h=null` minified); disableQueue and
// assertNativeAnimatedModule only run on the native-driver path, which RN never takes without the module, and a
// genuine "forced native driver" error there should still throw.
const FLUSH_QUEUE = new RegExp(
  String.raw`(?<head>(?:=>|function\s*[\w$]*\s*\([^)]*\))\s*\{\s*)?(?<cond>if\s*\(\s*)?${INVARIANT}(?=\s*[;,]\s*[\w$]+\s*=\s*null\b)`,
  'g',
);
// The one-time "useNativeDriver is not supported" warning (dev: a LogBox toast). JS fallback is intended here.
const NATIVE_DRIVER_WARN = /console\.warn\((?=\s*(['"])Animated: `useNativeDriver` is not supported because the native)/g;

// RN's Animated flushQueue (run by every Animated component's effect) throws without NativeAnimatedModule, although
// useNativeDriver otherwise falls back to JS when it is absent (OneSim ships no native driver): return early instead.
// At the function start that is a real early return; anywhere else the invariant becomes a no-op (`void 0`), which
// is equivalent since only native-driver operations fill the queue it would flush.
function patchAnimated(src) {
  let flushQueue = 0, warning = 0;
  const out = src
    .replace(FLUSH_QUEUE, (...a) => {
      const { head, cond, mod } = a[a.length - 1];
      flushQueue++;
      return `${head ? `${head}if(!${mod})return;` : ''}${cond || ''}void 0`;
    })
    .replace(NATIVE_DRIVER_WARN, () => { warning++; return 'void('; });
  return { src: out, flushQueue, warning, hasAnimated: src.includes(MSG) };
}

module.exports = { patchAnimated };
