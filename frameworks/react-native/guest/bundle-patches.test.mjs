// node --test frameworks/react-native/guest/bundle-patches.test.mjs
// patchAnimated on the NativeAnimatedHelper shapes Metro emits (RN 0.88): dev (`expo start`, `--no-minify`) and minified
// (`expo export`, iOS and Android's two batching arms), run with and without a NativeAnimatedModule.
// End to end on a real minified export: run-export.mjs.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
import vm from 'node:vm';

const { patchAnimated } = createRequire(import.meta.url)('./bundle-patches.js');
const MSG = 'Native animated module is not available';

// Each snippet defines API (flushQueue, disableQueue), assertNativeAnimatedModule and shouldUseNativeDriver over
// `mod` (NativeAnimatedModule), `queue` and `invariant`; `ran` records what flushQueue reached.
const shapes = {
  dev: `
  var invariant = { default: (c, m) => { if (!c) throw new Error('Invariant Violation: ' + m); } };
  var NativeAnimatedModule = mod, flushQueueImmediate = 1, queue = [];
  var _warnedMissingNativeAnimated = false;
  var API = {
    disableQueue() {
      (0, invariant.default)(NativeAnimatedModule, '${MSG}');
      ran.push('disable');
    },
    flushQueue: false ? () => {} : () => {
      (0, invariant.default)(NativeAnimatedModule, '${MSG}');
      flushQueueImmediate = null;
      ran.push('flush');
      if (queue.length === 0) {
        return;
      }
    }
  };
  function assertNativeAnimatedModule() {
    (0, invariant.default)(NativeAnimatedModule, '${MSG}');
  }
  function shouldUseNativeDriver(config) {
    if (config.useNativeDriver === true && !NativeAnimatedModule) {
      if (!_warnedMissingNativeAnimated) {
        console.warn('Animated: \`useNativeDriver\` is not supported because the native ' + 'animated module is missing.');
        _warnedMissingNativeAnimated = true;
      }
      return false;
    }
    return config.useNativeDriver;
  }`,
  // expo export -p ios: `if(<invariant>,h=null,0!==p.length){`
  minIos: `var u={default:(c,m)=>{if(!c)throw new Error('Invariant Violation: '+m)}},c=mod,h=1,p=[];var F=!1;
  var API={disableQueue(){(0,u.default)(c,'${MSG}'),ran.push('disable')},flushQueue:()=>{if((0,u.default)(c,'${MSG}'),h=null,ran.push('flush'),0!==p.length){c?.startOperationBatch?.()}}};
  function assertNativeAnimatedModule(){(0,u.default)(c,'${MSG}')}
  function shouldUseNativeDriver(e){return!0===e.useNativeDriver&&!c?(F||(console.warn("Animated: \`useNativeDriver\` is not supported because the native animated module is missing."),F=!0),!1):e.useNativeDriver}`,
  // expo export -p android: both arms (single-op batching and the queue), the first as a comma sequence.
  minAndroid: `var l={default:(c,m)=>{if(!c)throw new Error('Invariant Violation: '+m)}},f=mod,w=1,V=[],p=[],b=!!f;var S=!1;
  var API={disableQueue(){(0,l.default)(f,'${MSG}'),ran.push('disable')},flushQueue:b?()=>{(0,l.default)(f,'${MSG}'),w=null,ran.push('flush'),0!==V.length&&(f?.queueAndExecuteBatchedOperations?.(V),V.length=0)}:()=>{if((0,l.default)(f,'${MSG}'),w=null,ran.push('flush'),0!==p.length){f?.startOperationBatch?.()}}};
  function assertNativeAnimatedModule(){(0,l.default)(f,'${MSG}')}
  function shouldUseNativeDriver(e){return!0===e.useNativeDriver&&!f?(S||(console.warn("Animated: \`useNativeDriver\` is not supported because the native animated module is missing."),S=!0),!1):e.useNativeDriver}`,
  // Other minifier spellings: `function(){}` bodies, a bare invariant callee, double quotes, `;` separators.
  minVariant: `function inv(c,m){if(!c)throw new Error('Invariant Violation: '+m)}var M=mod,q=1,Q=[];
  var API={disableQueue:function(){inv(M,"${MSG}");ran.push('disable')},flushQueue:function(){inv(M,"${MSG}");q=null;ran.push('flush');if(Q.length)M.startOperationBatch()}};
  function assertNativeAnimatedModule(){inv(M,"${MSG}")}
  function shouldUseNativeDriver(e){return e.useNativeDriver&&!M?(console.warn('Animated: \`useNativeDriver\` is not supported because the native animated module is missing.'),!1):e.useNativeDriver}`,
};

const load = (src, mod) => {
  const warns = [];
  const ctx = vm.createContext({ mod, ran: [], console: { warn: (m) => warns.push(m) } });
  vm.runInContext(`${src}\nthis.API=API;this.assertNativeAnimatedModule=assertNativeAnimatedModule;this.shouldUseNativeDriver=shouldUseNativeDriver;`, ctx);
  return { ctx, warns };
};

for (const [name, src] of Object.entries(shapes)) {
  test(`${name}: unpatched flushQueue throws without the module (the bug)`, () => {
    assert.throws(() => load(src, null).ctx.API.flushQueue(), /Native animated module is not available/);
  });

  test(`${name}: patched flushQueue returns early without the module`, () => {
    const p = patchAnimated(src);
    assert.ok(p.hasAnimated);
    assert.equal(p.flushQueue, name === 'minAndroid' ? 2 : 1);
    assert.equal(p.warning, 1);
    const { ctx, warns } = load(p.src, null);
    ctx.API.flushQueue();
    ctx.API.flushQueue(); // every Animated component's effect calls it
    assert.deepEqual([...ctx.ran], [], 'returned before touching the queue');
    // The JS-driver fallback stays, without the LogBox warning.
    assert.equal(ctx.shouldUseNativeDriver({ useNativeDriver: true }), false);
    assert.equal(ctx.shouldUseNativeDriver({ useNativeDriver: false }), false);
    assert.deepEqual(warns, []);
  });

  test(`${name}: native-driver-only asserts still throw without the module`, () => {
    const { ctx } = load(patchAnimated(src).src, null);
    assert.throws(() => ctx.assertNativeAnimatedModule(), /Native animated module is not available/);
    assert.throws(() => ctx.API.disableQueue(), /Native animated module is not available/);
  });

  test(`${name}: patched flushQueue still flushes with a module`, () => {
    const mod = { startOperationBatch() {}, queueAndExecuteBatchedOperations() {} };
    const { ctx } = load(patchAnimated(src).src, mod);
    ctx.API.flushQueue();
    ctx.API.disableQueue();
    ctx.assertNativeAnimatedModule();
    assert.deepEqual([...ctx.ran], ['flush', 'disable']);
    assert.equal(ctx.shouldUseNativeDriver({ useNativeDriver: true }), true);
  });

  test(`${name}: patching is idempotent`, () => {
    const once = patchAnimated(src).src;
    const twice = patchAnimated(once);
    assert.equal(twice.src, once);
    assert.equal(twice.flushQueue, 0);
    assert.equal(twice.warning, 0);
  });
}

test('bundles without Animated pass through untouched', () => {
  for (const src of ['', '__d(function(){console.log("hi")},0,[]);', 'invariant(x, "Native animated")', 'שלום ✓ ' + 'x'.repeat(1 << 16)]) {
    const p = patchAnimated(src);
    assert.equal(p.src, src);
    assert.deepEqual([p.flushQueue, p.warning, p.hasAnimated], [0, 0, false]);
  }
});

test('an unrecognised flushQueue shape is reported, not silently mangled', () => {
  const src = `var API={flushQueue(){if(!c)throw Error('${MSG}')}}`;
  const p = patchAnimated(src);
  assert.equal(p.src, src);
  assert.deepEqual([p.flushQueue, p.hasAnimated], [0, true]); // host.js logs a warning for this
});
