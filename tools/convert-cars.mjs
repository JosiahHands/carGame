// Turns packs/graphics/models/cars.glb (meshopt-compressed, WebP textures)
// into a plain glTF binary the game's Bevy loader reads directly:
// assets/packs/photo_real/cars.glb. Run once after changing the models:
//
//   node tools/convert-cars.mjs
//
// Needs the meshoptimizer npm package and `dwebp` (from libwebp).
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { MeshoptDecoder } from 'meshoptimizer';

const input = process.argv[2] ?? 'packs/graphics/models/cars.glb';
const output = process.argv[3] ?? 'assets/packs/photo_real/cars.glb';

const glb = readFileSync(input);
const jsonLength = glb.readUInt32LE(12);
const json = JSON.parse(glb.subarray(20, 20 + jsonLength).toString('utf8'));
const binStart = 20 + jsonLength + 8;
const bin = glb.subarray(binStart, binStart + glb.readUInt32LE(20 + jsonLength));

await MeshoptDecoder.ready;
const chunks = [];
let offset = 0;
const add = (bytes) => {
  const at = offset;
  chunks.push(Buffer.from(bytes));
  offset += bytes.length;
  const pad = (4 - (offset % 4)) % 4;
  if (pad) {
    chunks.push(Buffer.alloc(pad));
    offset += pad;
  }
  return at;
};

// Every buffer view, decoded where it was compressed.
const views = json.bufferViews.map((view) => {
  const ext = view.extensions?.EXT_meshopt_compression;
  if (ext) {
    const source = bin.subarray(ext.byteOffset ?? 0, (ext.byteOffset ?? 0) + ext.byteLength);
    const out = new Uint8Array(ext.count * ext.byteStride);
    MeshoptDecoder.decodeGltfBuffer(out, ext.count, ext.byteStride, source, ext.mode, ext.filter ?? 'NONE');
    return { bytes: out, byteStride: view.byteStride, target: view.target };
  }
  if (view.buffer !== 0) return { bytes: new Uint8Array(0), byteStride: view.byteStride, target: view.target }; // a fallback buffer: not needed
  return { bytes: bin.subarray(view.byteOffset ?? 0, (view.byteOffset ?? 0) + view.byteLength), byteStride: view.byteStride, target: view.target };
});

// WebP pictures become PNG.
const work = mkdtempSync(join(tmpdir(), 'cars-'));
for (const [i, image] of (json.images ?? []).entries()) {
  if (image.mimeType !== 'image/webp') continue;
  const v = views[image.bufferView];
  const src = join(work, `${i}.webp`);
  const dst = join(work, `${i}.png`);
  writeFileSync(src, v.bytes);
  execFileSync('dwebp', [src, '-o', dst], { stdio: 'ignore' });
  v.bytes = readFileSync(dst);
  image.mimeType = 'image/png';
}
rmSync(work, { recursive: true });
for (const texture of json.textures ?? []) {
  const webp = texture.extensions?.EXT_texture_webp;
  if (webp) {
    texture.source = webp.source;
    delete texture.extensions.EXT_texture_webp;
    if (Object.keys(texture.extensions).length === 0) delete texture.extensions;
  }
}

// Small-integer normals and texture coordinates (packed by meshopt) become plain floats, which every loader reads.
const sizes = { 5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4 };
const counts = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 };
const unpacked = new Set();
for (const mesh of json.meshes) {
  for (const prim of mesh.primitives) {
    for (const [name, index] of Object.entries(prim.attributes)) {
      const a = json.accessors[index];
      if (a.componentType === 5126 || unpacked.has(index) || !(name === 'NORMAL' || name.startsWith('TEXCOORD'))) continue;
      unpacked.add(index);
      const v = views[a.bufferView];
      const n = counts[a.type];
      const size = sizes[a.componentType];
      const stride = v.byteStride ?? n * size;
      const data = new DataView(v.bytes.buffer, v.bytes.byteOffset, v.bytes.byteLength);
      const out = new Float32Array(a.count * n);
      for (let i = 0; i < a.count; i++) {
        for (let k = 0; k < n; k++) {
          const at = (a.byteOffset ?? 0) + i * stride + k * size;
          let x;
          switch (a.componentType) {
            case 5120: x = data.getInt8(at); x = a.normalized ? Math.max(x / 127, -1) : x; break;
            case 5121: x = data.getUint8(at); x = a.normalized ? x / 255 : x; break;
            case 5122: x = data.getInt16(at, true); x = a.normalized ? Math.max(x / 32767, -1) : x; break;
            case 5123: x = data.getUint16(at, true); x = a.normalized ? x / 65535 : x; break;
            default: x = data.getUint32(at, true);
          }
          out[i * n + k] = x;
        }
      }
      views.push({ bytes: new Uint8Array(out.buffer), target: 34962 });
      a.bufferView = views.length - 1;
      a.byteOffset = 0;
      a.componentType = 5126;
      delete a.normalized;
      if (a.min) a.min = undefined;
      if (a.max) a.max = undefined;
    }
  }
}

json.bufferViews = views.map((v) => {
  const out = { buffer: 0, byteOffset: add(v.bytes), byteLength: v.bytes.length };
  if (v.byteStride) out.byteStride = v.byteStride;
  if (v.target) out.target = v.target;
  return out;
});
json.buffers = [{ byteLength: offset }];
const drop = (list) => (list ?? []).filter((e) => e !== 'EXT_meshopt_compression' && e !== 'EXT_texture_webp');
json.extensionsUsed = drop(json.extensionsUsed);
json.extensionsRequired = drop(json.extensionsRequired);
if (!json.extensionsRequired.length) delete json.extensionsRequired;

const jsonBytes = Buffer.from(JSON.stringify(json));
const jsonPad = Buffer.alloc((4 - (jsonBytes.length % 4)) % 4, 0x20);
const binBytes = Buffer.concat(chunks);
const header = Buffer.alloc(12);
const total = 12 + 8 + jsonBytes.length + jsonPad.length + 8 + binBytes.length;
header.writeUInt32LE(0x46546c67, 0);
header.writeUInt32LE(2, 4);
header.writeUInt32LE(total, 8);
const chunkHead = (length, type) => {
  const b = Buffer.alloc(8);
  b.writeUInt32LE(length, 0);
  b.writeUInt32LE(type, 4);
  return b;
};
mkdirSync(join(output, '..'), { recursive: true });
writeFileSync(output, Buffer.concat([header, chunkHead(jsonBytes.length + jsonPad.length, 0x4e4f534a), jsonBytes, jsonPad, chunkHead(binBytes.length, 0x004e4942), binBytes]));
console.log(`${output}: ${(total / 1e6).toFixed(1)} MB`);
