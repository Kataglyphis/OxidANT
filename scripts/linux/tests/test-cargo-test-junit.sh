#!/usr/bin/env bash
# cargo-test-junit.py over recorded cargo test logs: every case lands with its outcome, and a crashed binary is a failure.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
convert="${tests_dir}/../cargo-test-junit.py"
fixtures="${tests_dir}/fixtures/cargo-test"

suite_run python3 "${convert}" "${fixtures}/verbose-pass-fail-crash.txt" "${work}/verbose.xml"
t_case "a verbose log with a failure, a crash and doc-tests converts"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" '^cargo-test-junit: 9 tests, 2 failed, 2 skipped in 3 binaries'
t_case "each binary is a suite named by its package and binary, the rustc lines name none"
t_assert_grep "${work}/verbose.xml" '<testsuite name="kataglyphis_core::kataglyphis_core" tests="4" failures="1" skipped="1" time="0.420">'
t_assert_grep "${work}/verbose.xml" '<testsuite name="kataglyphis_core::doc-tests" tests="2" failures="0" skipped="1" time="1.050">'
t_case "an ignored test keeps its reason, a failed one its captured output"
t_assert_grep "${work}/verbose.xml" '<skipped message="needs a Vulkan device" />'
t_assert_grep "${work}/verbose.xml" '^  left: 0$'
t_assert_grep "${work}/verbose.xml" "<failure message=\"thread 'config::tests::rejects_zero' panicked at crates/core/src/config.rs:88:9:\">"
t_case "a binary that died before its summary is one failure that names the signal"
t_assert_grep "${work}/verbose.xml" '<testsuite name="kataglyphis_gui::golden" tests="3" failures="1"'
t_assert_grep "${work}/verbose.xml" 'name="\(binary ended early\)"'
t_assert_grep "${work}/verbose.xml" '2 of 3 tests reported; .*SIGSEGV: invalid memory reference'

suite_run python3 "${convert}" "${fixtures}/plain-colour-slow.txt" "${work}/plain.xml"
t_case "the plain form names a suite by its binary and source, without colour codes or the slow-test notice"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/plain.xml" '<testsuite name="kataglyphis_text \(unittests src/lib.rs\)" tests="1"'
t_assert_grep "${work}/plain.xml" '<testcase classname="kataglyphis_text \(unittests src/lib.rs\)" name="tokenizer::tests::splits" />'
t_assert_grep "${work}/plain.xml" '<testsuite name="contract \(tests/contract.rs\)" tests="1" failures="0" skipped="0" time="64.310">'
t_assert_no_grep "${work}/plain.xml" 'has been running'

printf '   Compiling kataglyphis_core v0.1.0\nerror[E0425]: cannot find value x in this scope\n' > "${work}/build-failed.txt"
suite_run python3 "${convert}" "${work}/build-failed.txt" "${work}/none.xml"
t_case "a log in which no test binary ran is an error, not an empty record"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" 'no test binary ran'

t_summary
