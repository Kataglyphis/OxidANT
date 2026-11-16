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
const m = { colours: 1000, dominant: 0.4, changed: 0.05, sameSize: true, ...JSON.parse(process.argv[3]) };
const after = { drawn: step.mode, detail: '', ...JSON.parse(process.argv[4] || '{}') };
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
  { method: 'Runtime.exceptionThrown', params: { exceptionDetails: { text: 'Uncaught', exception: { description: 'RuntimeError: unreachable' } } } },
];
for (const e of events) console.log('PROBLEM ' + problemOf(e));
" "${check}"
t_case 'console errors, WebGPU warnings and exceptions count, a log line and the favicon do not'
t_assert_grep "${work}/out" '^PROBLEM console.error: panicked at render$'
t_assert_grep "${work}/out" '^PROBLEM rendering warning: Invalid ComputePipeline$'
t_assert_grep "${work}/out" '^PROBLEM exception: RuntimeError: unreachable$'
t_assert_eq "$(grep -c '^PROBLEM null$' "${work}/out")" 2

t_summary
