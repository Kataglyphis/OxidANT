#!/usr/bin/env node
// vision-core-web-contract.mjs <pkg dir> <inference-v1.json> <native report>: ModelANT's contract through the wasm module, checked, then compared with the native report bit for bit. See docs/testing.md#vision_core-in-the-browser
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [pkgDir, contractPath, nativePath] = process.argv.slice(2);
if (!nativePath) {
  console.error('usage: vision-core-web-contract.mjs <pkg dir> <inference-v1.json> <native report>');
  process.exit(2);
}

const wasm = await import(pathToFileURL(resolve(pkgDir, 'kataglyphis_vision_core_web.js')).href);
wasm.initSync({ module: readFileSync(resolve(pkgDir, 'kataglyphis_vision_core_web_bg.wasm')) });
const { Preprocessor, Mapping, decode, classNames } = wasm;

const contract = JSON.parse(readFileSync(contractPath, 'utf8'));
const report = {};
const failures = [];
const fail = (message) => failures.push(message);

// The bytes of a typed array, as the native report hashes each value's little-endian bytes.
const sha256 = (typed) => createHash('sha256')
  .update(Buffer.from(typed.buffer, typed.byteOffset, typed.byteLength)).digest('hex');
const f32Bits = (value) => new Uint32Array(Float32Array.of(value).buffer)[0];
const mappingBits = (m) => [m.srcWidth, m.srcHeight, f32Bits(m.scaleX), f32Bits(m.scaleY), f32Bits(m.padX), f32Bits(m.padY)];

// One case through Preprocessor: the tensor itself for the contract's checks, its hash for the native comparison.
function preprocessed(item, data, format) {
  const pre = new Preprocessor(item.input_width, item.input_height, item.mode);
  try {
    const mapping = pre.run(data, format, item.width, item.height, item.row_stride_bytes, item.plane_offset_bytes);
    const tensor = pre.tensor;
    const entry = { tensor: sha256(tensor), length: tensor.length, mapping: mappingBits(mapping) };
    mapping.free();
    return { tensor, mapping: entry.mapping, entry };
  } finally {
    pre.free();
  }
}

function checkMapping(name, item, bits) {
  const actual = { scale_x: bits[2], scale_y: bits[3], pad_x: bits[4], pad_y: bits[5] };
  for (const key of Object.keys(actual)) {
    const value = new Float32Array(new Uint32Array([actual[key]]).buffer)[0];
    if (Math.abs(value - item.mapping[key]) >= 1e-6) fail(`${name}: mapping ${key} ${value}, contract ${item.mapping[key]}`);
  }
}

for (const item of contract.cases) {
  const name = `tensor/${item.name}`;
  const { tensor, mapping, entry } = preprocessed(item, Uint8Array.from(item.data), item.pixel_format);
  report[name] = entry;
  if (tensor.length !== item.expected_nchw.length) fail(`${name}: ${tensor.length} values, contract ${item.expected_nchw.length}`);
  tensor.forEach((value, i) => {
    if (Math.abs(value - item.expected_nchw[i]) >= 1e-6) fail(`${name}: value ${i} is ${value}, contract ${item.expected_nchw[i]}`);
  });
  checkMapping(name, item, mapping);
}

// digest_cases.frame: each byte from x, y and the channel, 171 in every padding and offset byte.
function digestFrame(item) {
  const channels = item.pixel_format.length;
  const data = new Uint8Array(item.plane_offset_bytes + item.row_stride_bytes * item.height).fill(171);
  for (let y = 0; y < item.height; y += 1) {
    for (let x = 0; x < item.width; x += 1) {
      for (let c = 0; c < channels; c += 1) {
        data[item.plane_offset_bytes + y * item.row_stride_bytes + x * channels + c] = (7 * x + 13 * y + 61 * c) % 256;
      }
    }
  }
  return data;
}

function checkDigest(name, item, tensor, entry) {
  const plane = item.input_width * item.input_height;
  for (const [channel, y, x, expected] of item.samples) {
    const value = tensor[channel * plane + y * item.input_width + x];
    if (f32Bits(value) !== f32Bits(expected)) fail(`${name}: sample ${channel},${y},${x} is ${value}, contract ${expected}`);
  }
  if (entry.tensor !== item.expected_sha256) fail(`${name}: SHA-256 ${entry.tensor}, contract ${item.expected_sha256}`);
}

for (const item of contract.digest_cases.cases) {
  const data = digestFrame(item);
  const name = `digest/${item.name}`;
  const { tensor, mapping, entry } = preprocessed(item, data, item.pixel_format);
  report[name] = entry;
  checkDigest(name, item, tensor, entry);
  checkMapping(name, item, mapping);
  if (item.pixel_format !== 'RGB') continue;
  // The same frame with red and blue swapped, read as BGR: the same tensor.
  for (let y = 0; y < item.height; y += 1) {
    for (let x = 0; x < item.width; x += 1) {
      const at = item.plane_offset_bytes + y * item.row_stride_bytes + x * 3;
      [data[at], data[at + 2]] = [data[at + 2], data[at]];
    }
  }
  const bgr = preprocessed(item, data, 'BGR');
  report[`${name}/bgr`] = bgr.entry;
  checkDigest(`${name}/bgr`, item, bgr.tensor, bgr.entry);
}

// JSON has no NaN or infinity, so the contract spells them NaN, Infinity and -Infinity, which Number reads.
const number = (value) => (typeof value === 'string' ? Number(value) : value);
const [firstClass, lastClass] = contract.output.class_range;
const classCount = lastClass - firstClass + 1;

for (const item of contract.decode.cases) {
  const name = `decode/${item.name}`;
  const data = Float32Array.from(item.rows.flat().map(number));
  const m = item.mapping;
  const mapping = new Mapping(item.width, item.height, m.scale_x, m.scale_y, m.pad_x, m.pad_y);
  const found = decode([1, item.rows.length, 6], data, item.score_threshold, mapping, false, classCount);
  mapping.free();
  report[name] = { rows: sha256(found), count: found.length / 6 };
  if (found.length / 6 !== item.expected.length) fail(`${name}: ${found.length / 6} detections, contract ${item.expected.length}`);
  item.expected.forEach((want, i) => {
    const row = found.subarray(6 * i, 6 * i + 6);
    if (row[5] !== want.class) fail(`${name}: detection ${i} class ${row[5]}, contract ${want.class}`);
    if (Math.abs(row[4] - want.score) > contract.decode.score_tolerance) fail(`${name}: detection ${i} score ${row[4]}`);
    want.box.forEach((corner, k) => {
      if (Math.abs(row[k] - corner) > contract.decode.coordinate_tolerance) fail(`${name}: detection ${i} corner ${k} ${row[k]}, contract ${corner}`);
    });
  });
}

// What a refusal throws: an Error named by its kind.
function refused(call) {
  try {
    const value = call();
    value?.free?.();
    return null;
  } catch (error) {
    if (!(error instanceof Error)) return { thrown: String(error) };
    return { kind: error.name, message: error.message };
  }
}

// The same refusals examples/contract_report.rs provokes.
const frame = new Uint8Array(64).fill(7);
const pre = new Preprocessor(4, 4, 'stretch');
const zeroInput = new Preprocessor(0, 4, 'stretch');
const unit = new Mapping(100, 100, 1, 1, 0, 0);
const refusals = {
  'buffer-too-short': () => pre.run(frame, 'RGBA', 4, 2, 20, 29),
  'stride-shorter-than-row': () => pre.run(frame, 'RGB', 4, 2, 11, 0),
  'zero-size-frame': () => pre.run(new Uint8Array(0), 'RGB', 0, 2, 0, 0),
  'zero-size-input': () => zeroInput.run(frame, 'RGB', 4, 2, 0, 0),
  'unknown-pixel-format': () => pre.run(frame, 'YUV', 4, 2, 0, 0),
  'unknown-resize-mode': () => new Preprocessor(4, 4, 'fit'),
  'too-few-columns': () => decode([1, 4], new Float32Array(4), 0.5, unit, false, 80),
  'batch-not-one': () => decode([2, 1, 6], new Float32Array(12), 0.5, unit, false, 80),
  'unsupported-rank': () => decode([6], new Float32Array(6), 0.5, unit, false, 80),
  'shorter-than-shape': () => decode([2, 6], new Float32Array(6), 0.5, unit, false, 80),
};
for (const [name, call] of Object.entries(refusals)) {
  report[`refusal/${name}`] = refused(call);
  if (!report[`refusal/${name}`]?.kind) fail(`refusal/${name}: no typed Error thrown`);
}
pre.free();
zeroInput.free();
unit.free();

const names = classNames();
if (names.length !== 80 || names[15] !== 'cat') fail(`classNames: ${names.length} names, class 15 ${names[15]}`);
report.classNames = createHash('sha256').update(names.join('\n')).digest('hex');

// Every key and value of both reports, keys sorted, so JSON's key order does not count.
const canonical = (value) => JSON.stringify(value, (key, v) => (v && typeof v === 'object' && !Array.isArray(v)
  ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, v[k]])) : v));
const native = JSON.parse(readFileSync(nativePath, 'utf8'));
for (const key of new Set([...Object.keys(native), ...Object.keys(report)])) {
  if (canonical(native[key]) !== canonical(report[key])) {
    fail(`${key}: wasm ${canonical(report[key])}, native ${canonical(native[key])}`);
  }
}

if (failures.length) {
  for (const message of failures) console.error(`FAIL ${message}`);
  process.exit(1);
}
const count = (prefix) => Object.keys(report).filter((key) => key.startsWith(prefix)).length;
console.log(`vision_core_web on wasm32: ModelANT's contract holds, and ${Object.keys(report).length} results equal native bit for bit `
  + `(${count('tensor/')} tensor, ${count('digest/')} digest, ${count('decode/')} decode, ${count('refusal/')} refusal, the class names)`);
