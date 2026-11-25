#!/usr/bin/env node
// webgpu-demo-browser.js <web dir> [--frames DIR] [--size WxH]: the web demo's forward, deferred, clouds, ray-traced and path-traced frames in headless Chrome, each read back and checked. See docs/testing.md#the-web-demo-in-a-browser
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const hubScripts = path.join(process.env.ANTFRASTRUCTURE_DIR || path.join(__dirname, '../../third_party/ANTfrastructure'), 'linux/scripts');
const { findChrome, launch, serve, sleep } = require(path.join(hubScripts, 'check-docs-sidebars.js'));
const { install, WEBGPU_FLAGS } = require(path.join(hubScripts, 'webgpu-canvas-readback.js'));

// SwiftShader compiles the path-tracing kernel in about two minutes before its first sample.
const STEP_TIMEOUT_MS = 10 * 60 * 1000;
const READY_TIMEOUT_MS = 3 * 60 * 1000;
const FRAME_TIMEOUT_MS = 5 * 60 * 1000;

// Clouds must change `minChanged` of deferred; a traced mode must draw its own shadow (`signature`), as it changes only 2.6-3.2 % of forward.
const STEPS = [
  { name: 'forward', mode: 'raster', path: 'forward', clouds: false, fxaa: true },
  { name: 'deferred', mode: 'raster', path: 'deferred', clouds: false, fxaa: true },
  { name: 'clouds', mode: 'raster', path: 'deferred', clouds: true, differsFrom: 'deferred', minChanged: 0.01 },
  { name: 'ray traced', mode: 'ray', path: 'forward', clouds: false, differsFrom: 'forward', signature: 'ray' },
  { name: 'path traced', mode: 'path', path: 'forward', clouds: false, differsFrom: 'forward', signature: 'path', samples: 2 },
];
const CAMERA = { yaw: 45, pitch: 5 };
// A scene has thousands of colours where a cleared target has one; a pixel changed when a channel moved by more than this.
const MIN_COLOURS = 256;
const MAX_DOMINANT = 0.9;
// The cascades keep 35 % of the sun at strength 0.65, the shadow ray none: the ray tracer's sun shadow is this much darker than forward.
const RAY_SHADOW_RATIO = 0.75;
// That shadow's share of the viewport and its mean brightest channel, lit by the ambient term: 0.32 % and 105 on SwiftShader.
const MIN_RAY_SHADOW = 0.0005;
const MIN_RAY_SHADOW_LIGHT = 48;
// In that shadow the path tracer gathers sky and the cube's red bounce: 165 bright, (r + b) / 2 - g 28 over the old black mask, forward 0.8.
const MIN_PATH_SHADOW_LIGHT = 80;
const MIN_PATH_SHADOW_TINT = 10;
// The demo's one warning when the sky folder is not served: a face that answered 404.
const SKYBOX_MISSING = /Keeping the procedural sky: \S*kloofendal_43d_clear_puresky\/\S+: HTTP 404$/;

function fail(message, code = 2) {
  process.stderr.write(`webgpu-demo-browser: ${message}\n`);
  process.exit(code);
}

function parseArgs(argv) {
  const opts = { frames: '', size: '960x600' };
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const flag = { '--frames': 'frames', '--size': 'size' }[argv[i]];
    if (!flag) { rest.push(argv[i]); continue; }
    if (i + 1 >= argv.length) fail(`${argv[i]} needs a value`);
    opts[flag] = argv[++i];
  }
  const size = /^(\d+)x(\d+)$/.exec(opts.size);
  if (rest.length !== 1 || !size) fail('usage: webgpu-demo-browser.js <web dir> [--frames DIR] [--size WxH]');
  if (!fs.existsSync(path.join(rest[0], 'index.html')) || !fs.existsSync(path.join(rest[0], 'pkg'))) {
    fail(`${rest[0]} holds no index.html and pkg/: build the demo first (wasm-bindgen --target web --out-dir <web dir>/pkg)`);
  }
  return { dir: path.resolve(rest[0]), frames: opts.frames, width: Number(size[1]), height: Number(size[2]) };
}

// Every console error or warning, uncaught exception and browser log entry the page produced: none is expected.
function problemOf(m) {
  const p = m.params || {};
  // The cube sky comes from BeschleunigerBallett, which only its docs build stages beside the page; the procedural sky stays.
  if (m.method === 'Runtime.consoleAPICalled' && p.type === 'warning' && SKYBOX_MISSING.test(p.args.map((a) => a.value ?? '').join(' '))) return null;
  if (m.method === 'Runtime.consoleAPICalled' && ['error', 'warning', 'assert'].includes(p.type)) {
    return `console.${p.type}: ${p.args.map((a) => a.value ?? a.description ?? a.type).join(' ')}`;
  }
  // Chrome asks every page for a favicon the demo does not ship.
  if (m.method === 'Log.entryAdded' && /\/favicon\.ico$/.test(p.entry.url || '')) return null;
  if (m.method === 'Log.entryAdded' && /\/kloofendal_43d_clear_puresky\//.test(p.entry.url || '')) return null;
  if (m.method === 'Log.entryAdded' && ['error', 'warning'].includes(p.entry.level)) return `${p.entry.source} ${p.entry.level}: ${p.entry.text}${p.entry.url ? ` (${p.entry.url})` : ''}`;
  if (m.method === 'Runtime.exceptionThrown') return `exception: ${p.exceptionDetails.exception?.description ?? p.exceptionDetails.text}`;
  return null;
}

async function evaluate(browser, expression) {
  const answer = await browser.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  const { result, exceptionDetails } = answer.result || {};
  if (exceptionDetails) throw new Error(`${expression.slice(0, 60)}: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`);
  return result && result.value;
}

// A page busy compiling a kernel answers no call for a while; a probe that got no answer is tried again.
async function waitFor(what, timeoutMs, probe) {
  const start = Date.now();
  let last;
  while (Date.now() - start < timeoutMs) {
    try {
      last = await probe();
    } catch (e) {
      if (!/got no answer/.test(e.message)) throw e;
      last = { busy: e.message };
    }
    if (last.done) return last;
    if (last.error) throw new Error(`${what}: ${last.error}`);
    await sleep(500);
  }
  throw new Error(`${what}: not within ${timeoutMs / 1000} s, last ${JSON.stringify(last)}`);
}

// Runs in the page: the read-back's 2D copy of the frame, measured inside the scene viewport, kept for later diffs, then removed.
function measure(name, viewport, against, keepPng) {
  const overlay = document.body.lastElementChild;
  if (!(overlay instanceof HTMLCanvasElement)) return { error: 'the read-back left no frame copy' };
  const { width: w, height: h } = overlay;
  const data = overlay.getContext('2d').getImageData(0, 0, w, h).data;
  const png = keepPng ? overlay.toDataURL('image/png') : '';
  overlay.remove();
  const frames = (window.__demoFrames = window.__demoFrames || {});
  frames[name] = { w, h, data };
  // Two percent in from each edge keeps the dock borders out.
  const [x0, y0, x1, y1] = viewport.map((f, i) => Math.round(f * (i % 2 ? h : w)));
  const [ix, iy] = [Math.ceil((x1 - x0) * 0.02), Math.ceil((y1 - y0) * 0.02)];
  const base = against && frames[against];
  const seen = new Map();
  let changed = 0;
  let pixels = 0;
  for (let y = y0 + iy; y < y1 - iy; y++) {
    for (let x = x0 + ix; x < x1 - ix; x++) {
      const i = (y * w + x) * 4;
      const key = (data[i] << 16) | (data[i + 1] << 8) | data[i + 2];
      seen.set(key, (seen.get(key) || 0) + 1);
      if (base && base.w === w && base.h === h && [0, 1, 2].some((c) => Math.abs(data[i + c] - base.data[i + c]) > 8)) changed++;
      pixels++;
    }
  }
  const dominant = Math.max(0, ...seen.values()) / Math.max(1, pixels);
  const sameSize = !base || (base.w === w && base.h === h);
  return { width: w, height: h, pixels, colours: seen.size, dominant, changed: base && sameSize ? changed / pixels : null, sameSize, png };
}

// Runs in the page over the kept frames: the ray tracer's sun shadow, and what the path tracer and forward draw in it.
function shadowSignature(viewport, ratio) {
  const frames = window.__demoFrames || {};
  const [ray, path, forward] = [frames['ray traced'], frames['path traced'], frames.forward];
  if (!ray || !forward || ray.w !== forward.w || ray.h !== forward.h) return { error: 'no ray-traced and forward frames of one size' };
  const { w, h } = ray;
  const [x0, y0, x1, y1] = viewport.map((f, i) => Math.round(f * (i % 2 ? h : w)));
  const [ix, iy] = [Math.ceil((x1 - x0) * 0.02), Math.ceil((y1 - y0) * 0.02)];
  const peak = (data, i) => Math.max(data[i], data[i + 1], data[i + 2]);
  let [pixels, shadow, rayLight, light, tint, forwardTint] = [0, 0, 0, 0, 0, 0];
  for (let y = y0 + iy; y < y1 - iy; y++) {
    for (let x = x0 + ix; x < x1 - ix; x++) {
      const i = (y * w + x) * 4;
      pixels++;
      if (!(peak(ray.data, i) < peak(forward.data, i) * ratio)) continue;
      shadow++;
      rayLight += peak(ray.data, i);
      const f = forward.data;
      forwardTint += (f[i] + f[i + 2]) / 2 - f[i + 1];
      if (!path || path.w !== w || path.h !== h) continue;
      const p = path.data;
      light += Math.max(p[i], p[i + 1], p[i + 2]);
      tint += (p[i] + p[i + 2]) / 2 - p[i + 1];
    }
  }
  const mean = (sum) => (shadow ? sum / shadow : 0);
  return { shadow: shadow / pixels, rayLight: mean(rayLight), pathLight: path ? mean(light) : null, pathTint: path ? mean(tint) : null, forwardTint: mean(forwardTint) };
}

// The next frame's read-back, polled: one traced frame on SwiftShader can outlast a single DevTools call.
async function readFrame(browser) {
  await evaluate(browser, 'window.__demoFrame = null; window.__webgpuReadback().then((f) => { window.__demoFrame = f; }); true');
  const { frame } = await waitFor('the read-back', FRAME_TIMEOUT_MS, async () => {
    const f = await evaluate(browser, 'window.__demoFrame');
    return f && f.error ? { error: f.error } : { done: Boolean(f), frame: f };
  });
  return frame;
}

async function demoState(browser) {
  return evaluate(browser, 'window.__demo.demo_view()');
}

// The step's frame once the demo draws what it asked: one frame to settle, the next measured.
async function runStep(browser, step, opts) {
  await evaluate(browser, `window.__demo.set_demo_view(${JSON.stringify(step.mode)}, ${JSON.stringify(step.path)}, ${step.clouds})`);
  const state = await waitFor(`${step.name} drawn`, STEP_TIMEOUT_MS, async () => {
    const s = await demoState(browser);
    if (s.trace === 'failed') return { error: `the tracer failed: ${s.detail}` };
    const done = s.drawn === step.mode && s.path === step.path && s.clouds === step.clouds && s.samples >= (step.samples || 0);
    return { ...s, done };
  });
  await readFrame(browser);
  await evaluate(browser, 'document.body.lastElementChild.remove()');
  const frame = await readFrame(browser);
  const after = await waitFor(`${step.name} state`, FRAME_TIMEOUT_MS, async () => ({ ...(await demoState(browser)), done: true }));
  const m = await evaluate(browser, `(${measure})(${JSON.stringify(step.name)}, ${JSON.stringify(state.viewport)}, ${JSON.stringify(step.differsFrom || null)}, ${Boolean(opts.frames)})`);
  if (m.error) throw new Error(`${step.name}: ${m.error}`);
  if (step.signature) {
    m.signature = await evaluate(browser, `(${shadowSignature})(${JSON.stringify(state.viewport)}, ${RAY_SHADOW_RATIO})`);
    if (m.signature.error) throw new Error(`${step.name}: ${m.signature.error}`);
  }
  if (opts.frames) fs.writeFileSync(path.join(opts.frames, `${step.name.replace(/ /g, '-')}.png`), Buffer.from(m.png.split(',')[1], 'base64'));
  return { step, frame, after, m };
}

// The verdicts on one step: empty when it passes.
function verdicts({ step, frame, after, m }) {
  const out = [];
  if (after.drawn !== step.mode) out.push(`drew ${after.drawn} after the read-back, asked ${step.mode} (${after.detail})`);
  if (m.colours < MIN_COLOURS) out.push(`${m.colours} colours in the scene viewport, fewer than ${MIN_COLOURS}: nothing drawn`);
  if (m.dominant > MAX_DOMINANT) out.push(`one colour covers ${(m.dominant * 100).toFixed(1)} % of the scene viewport: a cleared target`);
  if (step.differsFrom && !m.sameSize) out.push(`the canvas changed size since ${step.differsFrom}`);
  if (step.minChanged && m.changed !== null && m.changed < step.minChanged) {
    out.push(`${(m.changed * 100).toFixed(2)} % of pixels differ from ${step.differsFrom}, under ${step.minChanged * 100} %: the ${step.name} pass drew nothing of its own`);
  }
  if (step.fxaa !== undefined && after.fxaa !== step.fxaa) out.push(`FXAA ${after.fxaa ? 'ran' : 'did not run'}, expected it ${step.fxaa ? 'on' : 'off'}`);
  out.push(...signatureVerdicts(step, m.signature));
  if (frame.format && !/^(rgba|bgra)8unorm/.test(frame.format)) out.push(`canvas format ${frame.format}`);
  return out;
}

// Each traced mode's own shadow: darker than forward's yet sky-lit for the ray tracer, sky-lit and red-tinted for the path tracer.
function signatureVerdicts(step, sig) {
  if (!step.signature) return [];
  if (!sig) return ['no shadow signature measured'];
  const out = [];
  const pct = (f) => `${(f * 100).toFixed(3)} %`;
  if (sig.shadow < MIN_RAY_SHADOW) out.push(`the ray tracer's sun shadow covers ${pct(sig.shadow)} of the viewport, under ${pct(MIN_RAY_SHADOW)}: no ray-traced signature`);
  else if (!(sig.rayLight >= MIN_RAY_SHADOW_LIGHT)) out.push(`the ray tracer's shadow is ${sig.rayLight?.toFixed(1)} bright, under ${MIN_RAY_SHADOW_LIGHT}: no ambient term lit it`);
  if (step.signature !== 'path') return out;
  if (!(sig.pathLight >= MIN_PATH_SHADOW_LIGHT)) out.push(`the path tracer's shadow is ${sig.pathLight?.toFixed(1)} bright, under ${MIN_PATH_SHADOW_LIGHT}: no sky light reached it`);
  if (!(sig.pathTint >= MIN_PATH_SHADOW_TINT)) out.push(`the path tracer's shadow is tinted ${sig.pathTint?.toFixed(1)}, under ${MIN_PATH_SHADOW_TINT}: no bounce from the cube`);
  return out;
}

function report({ step, frame, after, m }) {
  const changed = m.changed === null ? '' : `, ${(m.changed * 100).toFixed(1)} % changed against ${step.differsFrom}`;
  const samples = after.samples ? `, ${after.samples} spp` : '';
  const sig = m.signature;
  const shadow = sig ? `; shadow ${(sig.shadow * 100).toFixed(3)} %, ${sig.rayLight.toFixed(1)} bright${sig.pathLight === null ? '' : `, path-traced light ${sig.pathLight.toFixed(1)} tint ${sig.pathTint.toFixed(1)} (forward tint ${sig.forwardTint.toFixed(1)})`}` : '';
  return `${step.name}: drew ${after.drawn}/${after.path}${after.clouds ? '+clouds' : ''}${after.fxaa ? '+fxaa' : ''}${samples}, ${m.width}x${m.height} ${frame.format}, ${m.colours} colours, dominant ${(m.dominant * 100).toFixed(1)} %${changed}${shadow}`;
}

async function openDemo(browser, url, opts) {
  await browser.send('Log.enable');
  await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: `(${install})()` });
  await browser.send('Emulation.setDeviceMetricsOverride', { width: opts.width, height: opts.height, deviceScaleFactor: 1, mobile: false });
  await browser.send('Page.navigate', { url });
  await waitFor('the demo ready', READY_TIMEOUT_MS, async () => {
    const s = await evaluate(browser, "({ ready: !document.getElementById('scene').disabled, unsupported: !document.getElementById('unsupported').hidden, status: document.getElementById('status').textContent })");
    return s.unsupported ? { error: `the page fell back: ${s.status}` } : { ...s, done: s.ready };
  });
  // The page's own import of the same URL: one module instance, so these exports reach the running demo.
  await evaluate(browser, "import('./pkg/kataglyphis_webgpu_renderer.js').then((m) => { window.__demo = m; return true; })");
  // Low enough that the upper half of the scene is sky, where the clouds draw.
  await evaluate(browser, `window.__demo.set_demo_camera(${CAMERA.yaw}, ${CAMERA.pitch})`);
  // The browser presets steer the scale down on SwiftShader, which shrank the ray tracer's shadow under its floor.
  await evaluate(browser, 'window.__demo.set_demo_render_scale(1)');
}

async function run(opts) {
  const chrome = findChrome();
  if (!chrome) fail('no Chrome (CHROME_EXECUTABLE, chrome, chromium): the family image carries Chrome for Testing');
  if (opts.frames) fs.mkdirSync(opts.frames, { recursive: true });
  const problems = [];
  const server = await serve(opts.dir);
  const browser = await launch(chrome, WEBGPU_FLAGS, (m) => { const p = problemOf(m); if (p) problems.push(p); });
  const failures = [];
  try {
    process.stdout.write(`webgpu-demo-browser: ${browser.product}, ${opts.width}x${opts.height}\n`);
    await openDemo(browser, `http://127.0.0.1:${server.address().port}/index.html`, opts);
    for (const step of STEPS) {
      const started = Date.now();
      const result = await runStep(browser, step, opts);
      process.stdout.write(`  ${report(result)} (${((Date.now() - started) / 1000).toFixed(0)} s)\n`);
      failures.push(...verdicts(result).map((v) => `${step.name}: ${v}`));
    }
  } catch (e) {
    failures.push(e.message);
  } finally {
    try { await browser.close(); } finally { server.close(); }
  }
  failures.push(...problems);
  return failures;
}

async function main() {
  const failures = await run(parseArgs(process.argv.slice(2)));
  for (const f of failures) process.stderr.write(`webgpu-demo-browser: FAIL ${f}\n`);
  if (failures.length) process.exit(1);
  process.stdout.write('webgpu-demo-browser: every mode drew, no WebGPU error\n');
}

if (require.main === module) main().catch((e) => fail(e.stack || String(e)));
else module.exports = { STEPS, problemOf, verdicts, shadowSignature };
