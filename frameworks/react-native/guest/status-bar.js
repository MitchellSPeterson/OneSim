'use strict';

const ANIMATION_MS = 200; // UINavigationControllerHideShowBarDuration
const weights = (style) => [Number(style === 'light-content'), Number(style === 'default')];
const shown = [1, 0]; // opacity, upward displacement in bar heights
const hidden = [0, 0];

module.exports = function statusBar(rt, node, profileHeight, verticalEdge, emitVerticalEdge) {
  let visibility = null, tint = null, timer = null;
  let style = 'default', isHidden = false, verticalBehavior = 'automatic';
  const vertical = () => verticalEdge !== null && verticalBehavior === 'automatic';
  const height = () => vertical() || isHidden ? 0 : profileHeight();
  const sample = (motion, now) => {
    const t = Math.min(1, (now - motion.start) / ANIMATION_MS);
    const p = t * t * (3 - 2 * t);
    return motion.from.map((v, i) => v + (motion.to[i] - v) * p);
  };
  const update = () => {
    if (rt.dead) return;
    const now = node.performance.now();
    if (visibility && now - visibility.start >= ANIMATION_MS) visibility = null;
    if (tint && now - tint.start >= ANIMATION_MS) tint = null;
    if (visibility || tint) {
      const [opacity, slide] = visibility ? sample(visibility, now) : isHidden ? hidden : shown;
      const [light, automatic] = tint ? sample(tint, now) : weights(style);
      rt.rootProps.__simcoreStatusBarFrame = { opacity, slide, light, automatic };
      if (timer === null) timer = node.setTimeout(() => { timer = null; update(); }, 16);
    } else {
      delete rt.rootProps.__simcoreStatusBarFrame;
    }
    rt.scheduleEmit();
  };
  const changeFrame = (previousHeight) => {
    const nextHeight = height();
    if (previousHeight === nextHeight) return;
    const event = { frame: { x: 0, y: 0, width: nextHeight ? rt.window.width : 0, height: nextHeight } };
    rt.emit('statusBarFrameWillChange', event);
    rt.emit('statusBarFrameDidChange', event);
  };

  return {
    constants: () => ({ HEIGHT: verticalEdge === null ? profileHeight() : 0 }),
    getHeight: (callback) => callback({ height: height() }),
    setStyle(value, animated) {
      const next = value === 'light-content' || value === 'dark-content' ? value : 'default';
      if (style === next) return;
      const start = node.performance.now();
      tint = animated === true ? { start, from: tint ? sample(tint, start) : weights(style), to: weights(next) } : null;
      style = next;
      rt.rootProps.__simcoreStatusBarStyle = style;
      update();
    },
    setHidden(value, animation) {
      const next = !!value;
      if (isHidden === next) return;
      const previousHeight = height();
      const start = node.performance.now();
      visibility = !vertical() && (animation === 'fade' || animation === 'slide') ? {
        start,
        from: visibility ? sample(visibility, start) : !isHidden ? shown : animation === 'slide' ? [1, -1] : hidden,
        to: !next ? shown : animation === 'slide' ? [1, -1] : hidden,
      } : null;
      isHidden = next;
      rt.rootProps.__simcoreStatusBarHidden = isHidden;
      changeFrame(previousHeight);
      update();
    },
    getVerticalBarEdge: () => verticalEdge,
    setVerticalBarBehavior(behavior) {
      if (!['automatic', 'disabled'].includes(behavior)) throw new TypeError(`Invalid vertical bar behavior: ${behavior}`);
      if (verticalEdge === null || verticalBehavior === behavior) return;
      const previousHeight = height();
      verticalBehavior = behavior;
      visibility = null;
      rt.rootProps.__simcoreStatusBarVerticalDisabled = behavior === 'disabled';
      changeFrame(previousHeight);
      update();
    },
    // Expo posts the current trait after subscribing so the initial read and subscription cannot race.
    startObservingVerticalEdge: () => node.setImmediate(() => { if (!rt.dead) emitVerticalEdge({ edge: verticalEdge }); }),
  };
};
