#!/usr/bin/env bash
# The browser check's verdicts and console filter without a browser; the draw itself is web-browser.yml's.
set -u
# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
t_skip_unless "node" node --version

check="${tests_dir}/../webgpu-demo-browser.js"
judge() {
    suite_run node -e "
const { verdicts, problemOf, STEPS } = require(process.argv[1]);
const step = STEPS.find((s) => s.name === process.argv[2]);
const signature = { shadow: 0.0017, forwardDark: 0, pathLight: 165, pathTint: 28, forwardTint: 0.8 };
const given = JSON.parse(process.argv[3]);
const m = { colours: 1000, dominant: 0.4, changed: 0.05, sameSize: true, ...given, signature: { ...signature, ...given.signature } };
const after = { drawn: step.mode, fxaa: Boolean(step.fxaa), detail: '', ...JSON.parse(process.argv[4] || '{}') };
for (const v of verdicts({ step, frame: { format: 'rgba8unorm' }, after, m })) console.log('VERDICT ' + v);
console.log('DONE');
" "${check}" "$@"
}

judge 'path traced' '{}'
t_case 'a drawn, changed path-traced frame passes'
t_assert_no_grep "${work}/out" '^VERDICT'
t_assert_grep "${work}/out" '^DONE$'

judge 'forward' '{"colours": 1, "dominant": 1}'
t_case 'a cleared target fails on its colours and its dominant colour'
t_assert_grep "${work}/out" '^VERDICT 1 colours in the scene viewport'
t_assert_grep "${work}/out" '^VERDICT one colour covers 100.0 %'

judge 'clouds' '{"changed": 0.001}'
t_case 'clouds that change nothing against deferred fail'
t_assert_grep "${work}/out" '^VERDICT 0.10 % of pixels differ from deferred'

judge 'path traced' '{"changed": 0.001}'
t_case 'a traced frame is judged by its shadow, not by a share of changed pixels'
t_assert_no_grep "${work}/out" '^VERDICT'

judge 'ray traced' '{"signature": {"shadow": 0.0001}}'
t_case 'a ray-traced frame without its black shadow fails'
t_assert_grep "${work}/out" "^VERDICT the ray tracer's black sun shadow covers 0.010 % of the viewport, under 0.050 %$"

judge 'ray traced' '{"signature": {"forwardDark": 0.0017}}'
t_case 'a forward frame as dark as the ray-traced shadow tells nothing apart'
t_assert_grep "${work}/out" '^VERDICT forward is as dark .0.170 %. as the ray tracer'

judge 'path traced' '{"signature": {"pathLight": 32, "pathTint": 1}}'
t_case 'a path-traced shadow without sky light or bounce fails on both'
t_assert_grep "${work}/out" "^VERDICT the path tracer's shadow is 32.0 bright, under 80: no sky light reached it$"
t_assert_grep "${work}/out" "^VERDICT the path tracer's shadow is tinted 1.0, under 10: no bounce from the cube$"

judge 'deferred' '{}' '{"fxaa": false}'
t_case 'a deferred frame without FXAA fails'
t_assert_grep "${work}/out" '^VERDICT FXAA did not run, expected it on$'

suite_run node -e "
const { shadowSignature } = require(process.argv[1]);
// 6x6 frames, one pixel of the 4x4 inside the 2 % inset black in the ray-traced frame only.
const frame = (rgb) => ({ w: 6, h: 6, data: Uint8ClampedArray.from({ length: 144 }, (_, i) => (i % 4 === 3 ? 255 : rgb[i % 4])) });
const ray = frame([200, 200, 200]);
ray.data.set([10, 10, 10], (2 * 6 + 2) * 4);
const path = frame([150, 100, 130]);
global.window = { __demoFrames: { forward: frame([190, 190, 190]), 'ray traced': ray, 'path traced': path } };
console.log('SIGNATURE ' + JSON.stringify(shadowSignature([0, 0, 1, 1])));
" "${check}"
t_case 'the signature reads the ray-traced shadow and what the path tracer drew there'
t_assert_grep "${work}/out" '^SIGNATURE {"shadow":0.0625,"forwardDark":0,"pathLight":150,"pathTint":40,"forwardTint":0}$'

judge 'ray traced' '{}' '{"drawn": "raster", "detail": "accumulation does not fit"}'
t_case 'a traced pick that rastered fails and says why'
t_assert_grep "${work}/out" '^VERDICT drew raster after the read-back, asked ray .accumulation does not fit.$'

judge 'ray traced' '{"sameSize": false, "changed": null}'
t_case 'a canvas that changed size cannot be compared'
t_assert_grep "${work}/out" '^VERDICT the canvas changed size since forward$'

suite_run node -e "
const { problemOf } = require(process.argv[1]);
const events = [
  { method: 'Runtime.consoleAPICalled', params: { type: 'error', args: [{ type: 'string', value: 'panicked at render' }] } },
  { method: 'Runtime.consoleAPICalled', params: { type: 'log', args: [{ type: 'string', value: 'fine' }] } },
  { method: 'Log.entryAdded', params: { entry: { source: 'rendering', level: 'warning', text: 'Invalid ComputePipeline' } } },
  { method: 'Log.entryAdded', params: { entry: { source: 'network', level: 'error', text: '404', url: 'http://127.0.0.1:1/favicon.ico' } } },
  { method: 'Runtime.consoleAPICalled', params: { type: 'warning', args: [{ type: 'string', value: 'Keeping the procedural sky: kloofendal_43d_clear_puresky/px.png: HTTP 404' }] } },
  { method: 'Log.entryAdded', params: { entry: { source: 'network', level: 'error', text: '404', url: 'http://127.0.0.1:1/kloofendal_43d_clear_puresky/px.png' } } },
  { method: 'Runtime.consoleAPICalled', params: { type: 'warning', args: [{ type: 'string', value: 'Keeping the procedural sky: px.png: no PNG' }] } },
  { method: 'Runtime.exceptionThrown', params: { exceptionDetails: { text: 'Uncaught', exception: { description: 'RuntimeError: unreachable' } } } },
];
for (const e of events) console.log('PROBLEM ' + problemOf(e));
" "${check}"
t_case 'console errors, WebGPU warnings, exceptions and a broken sky count; a log line, the favicon and an unserved sky do not'
t_assert_grep "${work}/out" '^PROBLEM console.error: panicked at render$'
t_assert_grep "${work}/out" '^PROBLEM rendering warning: Invalid ComputePipeline$'
t_assert_grep "${work}/out" '^PROBLEM exception: RuntimeError: unreachable$'
t_assert_grep "${work}/out" '^PROBLEM console.warning: Keeping the procedural sky: px.png: no PNG$'
t_assert_eq "$(grep -c '^PROBLEM null$' "${work}/out")" 4

t_summary
