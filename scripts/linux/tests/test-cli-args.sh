#!/usr/bin/env bash
# The local option parser in a child shell; the scripts that call it keep their own wiring and defaults.
set -u
# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"

cat > "${work}/driver.sh" <<'DRIVER'
#!/usr/bin/env bash
set -euo pipefail
source "$1/../lib/cli-args.sh"
word="$2"
shift 2
show_help() { echo HELP; }
reject() { echo "REJECT[$1]"; exit 7; }
name=default flag=off
cli_args_parse "${word}" show_help reject --name=name --flag=flag=on -- "$@"
printf 'name=%s flag=%s\n' "${name}" "${flag}"
DRIVER

run() { suite_run bash "${work}/driver.sh" "${tests_dir}" "$@"; }

run 'a value'
t_case 'no arguments keep the defaults'
t_assert_grep "${work}/out" '^name=default flag=off$'
t_assert_grep "${work}/rc" '^0$'

run 'a value' --name 'two words' --flag
t_case 'a value keeps its spaces and a switch stores its mapped value'
t_assert_grep "${work}/out" '^name=two words flag=on$'

run 'a value' --name -h
t_case 'a value that looks like a flag is still the value'
t_assert_grep "${work}/out" '^name=-h flag=off$'

for missing in '--name' "--name|"; do
  IFS='|' read -r -a args <<< "${missing}"
  [ "${missing}" = '--name' ] || args+=('')
  run 'a value' "${args[@]}"
  t_case "a missing or empty value names the flag: ${missing}"
  t_assert_grep "${work}/out" '--name needs a value'
  t_assert_grep "${work}/rc" '^1$'
done

run '' --name
t_case 'without a word the shell default message remains'
t_assert_grep "${work}/out" 'parameter null or not set'
t_assert_grep "${work}/rc" '^1$'

for help in -h --help; do
  run 'a value' "${help}"
  t_case "${help} calls the help function and exits 0"
  t_assert_grep "${work}/out" '^HELP$'
  t_assert_grep "${work}/rc" '^0$'
done

run 'a value' --nope
t_case 'an unknown flag reaches the caller'
t_assert_grep "${work}/out" '^REJECT\[--nope\]$'
t_assert_grep "${work}/rc" '^7$'

run 'a value' ''
t_case 'an empty argument is unknown, not a map lookup'
t_assert_grep "${work}/out" '^REJECT\[\]$'

t_summary
