// expo-image natives (ExpoImage module and view functions) and the image sources beyond plain URLs that core Image and
// expo-image share: `ImageRef` shared refs (`Image.loadAsync`, `readFromCacheAsync`), `blurhash:/` and `thumbhash:/`
// sources and placeholders (decoded by the host), and the cache API. Blurhash / thumbhash generation and decoding run
// in the host (`request`, protocol section H), which has the image decoders.
'use strict';
const fs = require('node:fs');

module.exports = function createImages(rt, h) {
  const { node, loadImage, forgetImage, imageDone, readUri, SharedRef } = h;
  const refs = new Map(); // shared object id -> ImageRef
  let nextRef = 0;
  const MEDIA = { png: 'image/png', jpg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp' };
  const coded = (code, message) => Object.assign(new Error(message), { code });

  // expo-image's `Image` class (ImageModule.swift `Class(Image.self)`): a decoded image JS can hand back as a source.
  class ImageRef extends SharedRef {
    constructor(uri, info, size) {
      super();
      Object.defineProperty(this, '__expo_shared_object_id__', { value: ++nextRef }); // what a `source` prop carries
      Object.defineProperty(this, '__simcore', { value: { uri, info } });
      Object.assign(this, { width: size.width, height: size.height, scale: 1, isAnimated: !!info.animated, mediaType: MEDIA[info.path.split('.').pop()] || null });
      refs.set(nextRef, this);
    }
    get nativeRefType() { return 'image'; }
    release() { refs.delete(this.__expo_shared_object_id__); }
  }

  // `ImageLoadOptions` maxWidth / maxHeight: the ref reports the size the image was scaled down to (aspect kept).
  const fitted = (info, o) => {
    const k = Math.min(1, o && o.maxWidth > 0 ? o.maxWidth / info.width : 1, o && o.maxHeight > 0 ? o.maxHeight / info.height : 1);
    return { width: Math.round(info.width * k), height: Math.round(info.height * k) };
  };
  // A source as the native side receives it: an object ({uri, width, height, headers, cacheKey}), or a shared ref id.
  const resolve = (src) => {
    if (typeof src === 'number') { const r = refs.get(src); return r ? { uri: r.__simcore.uri, ref: r } : null; }
    if (src instanceof SharedRef && refs.has(src.__expo_shared_object_id__)) return { uri: src.__simcore.uri, ref: src };
    if (typeof src === 'string') return { uri: src };
    return src && src.uri ? src : null;
  };
  const load = (src) => {
    const s = resolve(src);
    if (!s) return Promise.reject(coded('ERR_IMAGE_SOURCE', 'The image source is not valid'));
    if (s.ref) return Promise.resolve(s.ref.__simcore.info);
    return loadImage(s.uri, s.headers, { key: s.cacheKey, width: s.width, height: s.height });
  };
  const components = (n) => (Array.isArray(n) ? { x: n[0], y: n[1] } : n && typeof n === 'object' ? { x: n.width, y: n.height } : { x: 4, y: 3 });
  const view = (tag) => rt.imageViews.get(tag) || (rt.imageViews.set(tag, {}), rt.imageViews.get(tag));

  const methods = (emit) => ({
    configureCache: () => {},
    prefetch: async (urls, cachePolicy, headers) => (await Promise.all([].concat(urls || []).map((u) => loadImage(String(u), headers || undefined).then(() => true, () => false)))).every(Boolean),
    generateBlurhashAsync: async (source, n) => {
      const info = await load(source).catch(() => { throw coded('ERR_BLURHASH_GENERATION', 'Unable to generate blurhash, make sure the image exists at the given URL'); });
      return rt.request('image.blurhash', { path: info.path, ...components(n) });
    },
    generateThumbhashAsync: async (source) => {
      const info = await load(source).catch(() => { throw coded('ERR_THUMBHASH_GENERATION', 'Unable to generate thumbhash, make sure the image exists at the given URL'); });
      return rt.request('image.thumbhash', { path: info.path });
    },
    // The decoded images and the files stay (views showing them keep them); the cache forgets them, so the next load
    // reads them again.
    clearMemoryCache: async () => true,
    clearDiskCache: async () => { forgetImage(); return true; },
    getCachePathAsync: async (key) => (imageDone.has(String(key)) ? imageDone.get(String(key)).path : null),
    writeToCacheAsync: async (source, key) => {
      const s = resolve(source);
      if (!s) throw coded('ERR_WRITE_TO_CACHE_SOURCE', 'The source must be an image reference or a file URL');
      if (!s.ref && !/^file:/.test(s.uri)) throw coded('ERR_WRITE_TO_CACHE_REMOTE_SOURCE', `Only local file URLs can be written to the cache, got: ${s.uri}`);
      const bytes = s.ref ? fs.readFileSync(s.ref.__simcore.info.path) : await readUri(s.uri);
      await loadImage(`data:;base64,${node.Buffer.from(bytes).toString('base64')}`, undefined, { key: String(key) });
    },
    readFromCacheAsync: async (key) => {
      const info = imageDone.get(String(key));
      return info ? new ImageRef(String(key), info, { width: info.width, height: info.height }) : null;
    },
    loadAsync: async (source, options) => {
      const s = resolve(source);
      const info = await load(source);
      const size = fitted(info, options);
      emit('imageLoaded', { url: s.uri, width: info.width, height: info.height });
      return new ImageRef(s.uri, info, size);
    },
  });

  // View functions (`ref.startAnimating()` ...): expo-modules-core calls them with `this.nativeTag` set.
  const viewPrototypes = {
    ExpoImage: {
      startAnimating: async function () { view(this.nativeTag).animating = true; rt.scheduleEmit(); },
      stopAnimating: async function () { view(this.nativeTag).animating = false; rt.scheduleEmit(); },
      // A locked view keeps showing what it loaded when its source changes.
      lockResourceAsync: async function () { view(this.nativeTag).locked = true; },
      unlockResourceAsync: async function () { view(this.nativeTag).locked = false; rt.scheduleEmit(); },
      // Loads the source again (from the network for a URL), with its load events.
      reloadAsync: async function () { const st = rt.images.get(this.nativeTag); if (st) forgetImage(st.key); rt.images.delete(this.nativeTag); rt.scheduleEmit(); },
    },
  };

  return { ImageRef, resolve, methods, viewPrototypes, media: (info) => MEDIA[info.path.split('.').pop()] || null };
};
