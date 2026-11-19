'use strict';
// expo-file-system for the OneSim guest: the real file system, confined to the session sandbox.
//   FileSystem        = the SDK 54+ object API (File / Directory / file handles; ios/FileSystemModule.swift)
//   ExponentFileSystem = the legacy `expo-file-system/legacy` string API (core methods only)
// Only <dataDir>/{Documents,Caches,Bundle} resolve (Bundle is read-only); any other path is "Missing permission", as on iOS.
// Async natives run the sync node:fs call inside an async function (promise semantics, no worker threads).
// Not provided (the caller marks them unsupported): network tasks, downloads, pickers, Quick Look preview, directory watching.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { URL, fileURLToPath, pathToFileURL } = require('node:url'); // node's own: RN's polyfills replace the globals Buffer and URL
const { Buffer } = require('node:buffer');

module.exports = function createFileSystem({ cfg, unsupported, expo: { SharedObject } }) {
  const dir = (n) => { const d = path.join(path.resolve(cfg.dataDir), n); fs.mkdirSync(d, { recursive: true }); return d; };
  const [docs, caches, bundle] = ['Documents', 'Caches', 'Bundle'].map(dir);
  const uriOf = (p, isDir) => pathToFileURL(p).href.replace(/\/*$/, isDir ? '/' : '');
  const within = (p, r) => p === r || p.startsWith(r + path.sep);
  const nope = (m) => unsupported('FileSystem', m, 'expo-file-system');

  // The trust boundary: URI -> absolute path, or throw.
  const local = (u, write) => {
    let p;
    try { p = path.resolve(fileURLToPath(u)); } catch (_) { throw new Error(`Missing permission for uri: ${u}`); }
    if (![docs, caches, bundle].some((r) => within(p, r)) || (write && within(p, bundle))) throw new Error(`Missing permission for uri: ${u}`);
    return p;
  };
  const buf = (c, enc) => (typeof c === 'string' ? Buffer.from(c, enc === 'base64' ? 'base64' : 'utf8') : ArrayBuffer.isView(c) ? Buffer.from(c.buffer, c.byteOffset, c.byteLength) : Buffer.from(c));
  const u8 = (b) => new Uint8Array(b); // copy: a view onto a Buffer pool would leak neighbouring bytes
  const attempt = (f) => { try { return f(); } catch (_) { return null; } };
  const fail = (what, e) => new Error(`Unable to ${what}: ${e.message}`);
  // ponytail: a handful of common types, not the full UTType table.
  const MIME = { txt: 'text/plain', md: 'text/markdown', json: 'application/json', html: 'text/html', css: 'text/css', js: 'text/javascript', csv: 'text/csv', xml: 'application/xml', pdf: 'application/pdf',
    png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml', mp3: 'audio/mpeg', mp4: 'video/mp4', zip: 'application/zip' };
  const HASH = { MD5: 'md5', 'SHA-1': 'sha1', 'SHA-256': 'sha256', 'SHA-384': 'sha384', 'SHA-512': 'sha512' };
  const du = (p) => fs.readdirSync(p, { recursive: true }).reduce((n, f) => n + (attempt(() => { const s = fs.statSync(path.join(p, f)); return s.isFile() ? s.size : 0; }) || 0), 0);

  class Path extends SharedObject {
    constructor(u, isDir) { super(); const x = new URL(u); x.pathname = x.pathname.replace(/\/*$/, isDir ? '/' : ''); this._u = x.href; this._d = isDir; }
    get uri() { return this._u; }
    get exists() { return !!attempt(() => fs.statSync(local(this._u)).isDirectory() === this._d); }
    validatePath() { if (!this._u.startsWith('file:')) throw new Error(`tried to create a ${this._d ? 'directory' : 'file'} with a non-file URI`); }
    get modificationTime() { return attempt(() => Math.trunc(fs.statSync(local(this._u)).mtimeMs)); }
    get creationTime() { return attempt(() => Math.trunc(fs.statSync(local(this._u)).birthtimeMs)); }
    delete() {
      const p = local(this._u, true);
      if (!fs.existsSync(p)) throw new Error('Unable to delete file or directory: path does not exist');
      try { fs.rmSync(p, { recursive: true }); } catch (e) { throw fail('delete file or directory', e); }
    }
    _dest(to) {
      const p = local(this._u), t = local(to._u, true);
      if (this._d && !to._d) throw new Error('Copying or moving a directory to a file is not supported');
      const d = !to._d || (this._d && !to.exists) ? t : path.join(t, path.basename(p));
      return [p, d];
    }
    _relocate(to, o, copy) {
      const [p, d] = this._dest(to);
      if (o && o.overwrite) fs.rmSync(d, { recursive: true, force: true });
      else if (fs.existsSync(d)) throw new Error('Destination already exists');
      if (copy) fs.cpSync(p, d, { recursive: true }); else { fs.renameSync(p, d); this._u = uriOf(d, this._d); }
    }
    async copy(to, o) { this._relocate(to, o, true); }
    copySync(to, o) { this._relocate(to, o, true); }
    async move(to, o) { this._relocate(to, o, false); }
    moveSync(to, o) { this._relocate(to, o, false); }
    rename(name) { const p = local(this._u, true), d = path.join(path.dirname(p), name); local(uriOf(d), true); fs.renameSync(p, d); this._u = uriOf(d, this._d); }
  }

  class FileSystemFile extends Path {
    constructor(u) { super(u, false); }
    create(o = {}) {
      const p = local(this._u, true);
      if (!o.overwrite && this.exists) throw new Error(`File '${this._u}' already exists`);
      try {
        if (o.intermediates) fs.mkdirSync(path.dirname(p), { recursive: true });
        fs.rmSync(p, { force: true });
        fs.writeFileSync(p, '');
      } catch (e) { throw fail('create file or directory', e); }
    }
    textSync() { return fs.readFileSync(local(this._u), 'utf8'); }
    base64Sync() { return fs.readFileSync(local(this._u)).toString('base64'); }
    bytesSync() { return u8(fs.readFileSync(local(this._u))); }
    async text() { return this.textSync(); }
    async base64() { return this.base64Sync(); }
    async bytes() { return this.bytesSync(); }
    writeSync(c, o) { (o && o.append ? fs.appendFileSync : fs.writeFileSync)(local(this._u, true), buf(c, o && o.encoding)); }
    async write(c, o) { this.writeSync(c, o); }
    open(mode) { return new FileSystemFileHandle(this, mode); }
    async digest(alg) {
      if (!HASH[alg]) throw new Error(`Unsupported digest algorithm: ${alg}`);
      return crypto.createHash(HASH[alg]).update(fs.readFileSync(local(this._u))).digest('hex');
    }
    info(o) {
      local(this._u);
      if (!this.exists) return { exists: false, uri: this._u };
      const r = { exists: true, uri: this._u, size: this.size, modificationTime: this.modificationTime, creationTime: this.creationTime };
      if (o && o.md5) r.md5 = this.md5;
      return r;
    }
    get size() { return attempt(() => fs.statSync(local(this._u)).size) || 0; }
    get md5() { return attempt(() => crypto.createHash('md5').update(fs.readFileSync(local(this._u))).digest('hex')); }
    get lastModified() { return this.modificationTime; }
    get type() { return MIME[path.extname(this._u).slice(1).toLowerCase()] || null; }
    async canPreview() { throw nope('File.canPreview'); }
    async preview() { throw nope('File.preview'); }
  }
  class FileSystemDirectory extends Path {
    constructor(u) { super(u, true); }
    create(o = {}) {
      const p = local(this._u, true);
      if (this.exists) { if (o.idempotent) return; if (!o.overwrite) throw new Error(`File '${this._u}' already exists`); }
      try { fs.mkdirSync(p, { recursive: !!o.intermediates }); } catch (e) { throw fail('create file or directory', e); }
    }
    listAsRecords() {
      const p = local(this._u);
      return fs.readdirSync(p, { withFileTypes: true }).map((e) => ({ isDirectory: e.isDirectory(), uri: uriOf(path.join(p, e.name), e.isDirectory()) }));
    }
    createFile(name) { const f = new FileSystemFile(uriOf(path.join(local(this._u), name))); f.create(); return f; }
    createDirectory(name) { const d = new FileSystemDirectory(uriOf(path.join(local(this._u), name), true)); d.create(); return d; }
    info() {
      const p = local(this._u);
      if (!this.exists) return { exists: false, uri: this._u };
      return { exists: true, uri: this._u, size: this.size, files: fs.readdirSync(p), modificationTime: this.modificationTime, creationTime: this.creationTime };
    }
    get size() { return attempt(() => du(local(this._u))); }
  }

  // A descriptor plus our own offset (FileHandle semantics: reads/writes advance it, `offset` seeks).
  class FileSystemFileHandle extends SharedObject {
    constructor(file, mode) {
      super();
      const m = mode || (attempt(() => local(file._u, true)) ? 'rw' : 'r');
      this._m = m;
      this._fd = fs.openSync(local(file._u, m !== 'r'), m === 'r' ? 'r' : 'r+'); // writing needs an existing file, like FileHandle(forWritingTo:)
      this._pos = 0;
      if (m === 'wa') this._pos = fs.fstatSync(this._fd).size;
      if (m === 'wt') fs.ftruncateSync(this._fd, 0);
    }
    readBytesSync(n) {
      if (['w', 'wa', 'wt'].includes(this._m)) throw new Error('Unable to read from a file handle: File opened write-only');
      const b = Buffer.alloc(n), k = fs.readSync(this._fd, b, 0, n, this._pos);
      this._pos += k;
      return u8(b.subarray(0, k));
    }
    writeBytesSync(bytes) {
      if (this._m === 'r') throw new Error('Unable to write to a file handle: File opened read-only');
      const b = buf(bytes);
      this._pos += fs.writeSync(this._fd, b, 0, b.length, this._pos);
    }
    async readBytes(n) { return this.readBytesSync(n); }
    async writeBytes(b) { this.writeBytesSync(b); }
    close() { if (this._fd != null) { fs.closeSync(this._fd); this._fd = null; } }
    get offset() { return this._fd == null ? null : this._pos; }
    set offset(v) { this._pos = Number(v); }
    get size() { return this._fd == null ? null : fs.fstatSync(this._fd).size; }
  }

  const noClass = (name) => class extends SharedObject { constructor() { super(); throw nope(name); } };
  const space = attempt(() => fs.statfsSync(docs)) || { bsize: 0, blocks: 0, bavail: 0 };

  // ---- legacy API (`ExponentFileSystem`): URI strings in, promises out ----
  const lstat = (p) => fs.statSync(p);
  const legacy = {
    getInfoAsync: async (u, o) => {
      const p = local(u), s = attempt(() => lstat(p));
      if (!s) return { exists: false, isDirectory: false };
      const r = { exists: true, isDirectory: s.isDirectory(), uri: uriOf(p, s.isDirectory()), size: s.isDirectory() ? du(p) : s.size, modificationTime: s.mtimeMs / 1000 };
      if (o && o.md5 && !s.isDirectory()) r.md5 = crypto.createHash('md5').update(fs.readFileSync(p)).digest('hex');
      return r;
    },
    readAsStringAsync: async (u, o = {}) => {
      const p = local(u);
      if (!fs.existsSync(p)) throw new Error(`File '${p}' does not exist`);
      let b = fs.readFileSync(p);
      if (o.position || o.length != null) b = b.subarray(o.position || 0, o.length != null ? (o.position || 0) + o.length : undefined);
      return b.toString(o.encoding === 'base64' ? 'base64' : 'utf8');
    },
    writeAsStringAsync: async (u, s, o = {}) => {
      const p = local(u, true);
      try { (o.append ? fs.appendFileSync : fs.writeFileSync)(p, buf(s, o.encoding)); } catch (_) { throw new Error(`File '${p}' is not writable`); }
    },
    deleteAsync: async (u, o = {}) => {
      const p = local(u, true);
      if (!fs.existsSync(p)) { if (o.idempotent) return; throw new Error(`File '${p}' could not be deleted`); }
      fs.rmSync(p, { recursive: true });
    },
    moveAsync: async ({ from, to }) => { const t = local(to, true); fs.rmSync(t, { recursive: true, force: true }); fs.renameSync(local(from, true), t); },
    copyAsync: async ({ from, to }) => { const f = local(from), t = local(to, true); fs.rmSync(t, { recursive: true, force: true }); fs.cpSync(f, t, { recursive: true }); },
    makeDirectoryAsync: async (u, o = {}) => { fs.mkdirSync(local(u, true), { recursive: !!o.intermediates }); },
    readDirectoryAsync: async (u) => fs.readdirSync(local(u)),
    getFreeDiskStorageAsync: async () => space.bsize * space.bavail,
    getTotalDiskCapacityAsync: async () => space.bsize * space.blocks,
  };

  const where = { documentDirectory: uriOf(docs, true), cacheDirectory: uriOf(caches, true), bundleDirectory: uriOf(bundle, true) };
  return {
    // disk space is a property natively; here a snapshot taken when the module is first used
    constants: () => ({ ...where, appleSharedContainers: {}, totalDiskSpace: space.bsize * space.blocks, availableDiskSpace: space.bsize * space.bavail }),
    methods: {
      info: (u) => { const s = attempt(() => lstat(local(u))); return s ? { exists: true, isDirectory: s.isDirectory() } : { exists: false }; },
      FileSystemFile, FileSystemDirectory, FileSystemFileHandle,
      FileSystemUploadTask: noClass('FileSystemUploadTask'), FileSystemDownloadTask: noClass('FileSystemDownloadTask'), FileSystemWatcher: noClass('FileSystemWatcher'),
    },
    legacyConstants: where,
    legacy,
  };
};
