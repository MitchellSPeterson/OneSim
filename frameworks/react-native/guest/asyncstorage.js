'use strict';
// @react-native-async-storage/async-storage (RNCAsyncStorage): one JSON file in the session data dir, so values survive reloads like the app's
// on-disk store. Callback API as the native module's: cb(errors | null, result).
const fs = require('node:fs');
const path = require('node:path');

module.exports = ({ cfg }) => {
  const file = path.join(cfg.dataDir, 'asyncstorage.json');
  const read = () => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch (_) { return {}; } };
  const write = (o) => fs.writeFileSync(file, JSON.stringify(o));
  const isObj = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
  const merge = (a, b) => { const o = { ...a }; for (const k of Object.keys(b)) o[k] = isObj(a[k]) && isObj(b[k]) ? merge(a[k], b[k]) : b[k]; return o; };
  const update = (cb, f) => { const o = read(); f(o); write(o); cb(null); };
  return [{ name: 'RNCAsyncStorage', package: '@react-native-async-storage/async-storage', status: 'implemented', methods: {
    multiGet: (keys, cb) => { const o = read(); cb(null, keys.map((k) => [k, k in o ? o[k] : null])); },
    multiSet: (kv, cb) => update(cb, (o) => kv.forEach(([k, v]) => { o[k] = v; })),
    multiRemove: (keys, cb) => update(cb, (o) => keys.forEach((k) => { delete o[k]; })),
    multiMerge: (kv, cb) => update(cb, (o) => kv.forEach(([k, v]) => { o[k] = k in o ? JSON.stringify(merge(JSON.parse(o[k]), JSON.parse(v))) : v; })),
    getAllKeys: (cb) => cb(null, Object.keys(read())),
    clear: (cb) => update(cb, (o) => { for (const k of Object.keys(o)) delete o[k]; }),
  } }];
};
