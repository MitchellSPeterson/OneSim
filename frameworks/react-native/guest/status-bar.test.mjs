import assert from 'node:assert/strict';
import test from 'node:test';
import statusBar from './status-bar.js';

function session(profileHeight, edge = null) {
  let now = 0;
  const pending = [];
  const events = [];
  const rt = { rootProps: {}, window: { width: 402 }, scheduleEmit() {}, emit: (...event) => events.push(event) };
  const node = { performance: { now: () => now }, setTimeout: (fn) => pending.push(fn), setImmediate: (fn) => pending.push(fn) };
  const bar = statusBar(rt, node, () => profileHeight, edge, (event) => events.push(['edge', event]));
  return { bar, events, props: rt.rootProps, advance(ms) { now += ms; pending.splice(0).forEach((fn) => fn()); },
    height: () => { let result; bar.getHeight(({ height }) => { result = height; }); return result; } };
}

test('iOS status bar height follows the profile and reports horizontal visibility changes', () => {
  for (const profile of [20, 44, 47, 59, 62]) {
    const { bar, events, props, height } = session(profile);
    assert.deepEqual(bar.constants(), { HEIGHT: profile });
    assert.equal(height(), profile);
    bar.setHidden(true, 'none');
    assert.equal(props.__simcoreStatusBarHidden, true);
    assert.equal(props.__simcoreStatusBarFrame, undefined);
    assert.equal(height(), 0);
    assert.deepEqual(events.splice(0), ['statusBarFrameWillChange', 'statusBarFrameDidChange']
      .map((name) => [name, { frame: { x: 0, y: 0, width: 0, height: 0 } }]));
    bar.setHidden(false, 'invalid');
    assert.equal(props.__simcoreStatusBarHidden, false);
    assert.equal(height(), profile);
    assert.equal(events[1][1].frame.width, 402);
  }
});

test('style and visibility run independently and a rapid reversal starts at the displayed frame', () => {
  const { bar, props, advance } = session(62);
  const frame = () => props.__simcoreStatusBarFrame;
  bar.setStyle('light-content', true);
  bar.setHidden(true, 'fade');
  advance(100);
  assert.deepEqual(frame(), { opacity: 0.5, slide: 0, light: 0.5, automatic: 0.5 });
  bar.setHidden(false, 'slide');
  assert.equal(frame().opacity, 0.5);
  bar.setStyle('dark-content', false);
  assert.deepEqual(frame(), { opacity: 0.5, slide: 0, light: 0, automatic: 0 });
  advance(201);
  assert.equal(frame(), undefined);
  bar.setHidden(true, 'slide');
  advance(100);
  assert.deepEqual(frame(), { opacity: 1, slide: -0.5, light: 0, automatic: 0 });
  advance(101);
  assert.equal(frame(), undefined);
  bar.setHidden(false, 'slide');
  assert.equal(frame().slide, -1);
  bar.setHidden(true, 'none');
  assert.equal(frame(), undefined);
  bar.setStyle(null, false);
  assert.equal(props.__simcoreStatusBarStyle, 'default');
});

test('Duo keeps its vertical edge while disabled and only hides the horizontal bar', () => {
  const { bar, events, props, height, advance } = session(24, 'trailing');
  assert.equal(bar.getVerticalBarEdge(), 'trailing');
  assert.equal(height(), 0);
  bar.startObservingVerticalEdge();
  advance(0);
  assert.deepEqual(events, [['edge', { edge: 'trailing' }]]);
  bar.setHidden(true, 'fade');
  assert.equal(props.__simcoreStatusBarFrame, undefined);
  bar.setVerticalBarBehavior('disabled');
  assert.equal(props.__simcoreStatusBarVerticalDisabled, true);
  assert.equal(bar.getVerticalBarEdge(), 'trailing');
  assert.equal(height(), 0);
  bar.setHidden(false, 'none');
  assert.equal(height(), 24);
  bar.setVerticalBarBehavior('automatic');
  assert.equal(props.__simcoreStatusBarVerticalDisabled, false);
  assert.equal(height(), 0);
  assert.throws(() => bar.setVerticalBarBehavior('invalid'), TypeError);
});
