#!/usr/bin/env bash
# check-slang-imports.py on fixture trees: each kind of top-level name counts as a use, and a dead import fails by name.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
gate="${tests_dir}/../../check-slang-imports.py"
tree="${work}/shaders"
mkdir -p "${tree}/common" "${tree}/pass"

cat > "${tree}/common/lib.slang" <<'SLANG'
// A comment naming unused_in_comment must not count.
static const float LIB_SCALE = 2.0;
[vk::binding(0, 0)] RWStructuredBuffer<float4> libBuffer;
struct LibData : ILib { float x; };
interface ILib { float value(); };
#define LIB_MACRO 1
float lib_fn(float x) { return x * LIB_SCALE; }
SLANG
cat > "${tree}/common/reexport.slang" <<'SLANG'
__exported import lib;
float reexport_fn() { return 1.0; }
SLANG

# use <name> <body>: a pass importing lib whose only reference is <body>.
use() {
    printf 'import lib;\n[shader("compute")]\n[numthreads(1, 1, 1)]\nvoid main() { %s }\n' "$2" > "${tree}/pass/$1.slang"
}

for case in "constant:float y = LIB_SCALE;" "buffer:libBuffer[0] = float4(0.0);" "struct:LibData d;" \
    "macro:int m = LIB_MACRO;" "function:float y = lib_fn(1.0);"; do
    rm -f "${tree}"/pass/*.slang
    use "${case%%:*}" "${case#*:}"
    t_case "a ${case%%:*} the module declares is a use"
    t_assert_rc 0 python3 "$gate" "${tree}"
done

rm -f "${tree}"/pass/*.slang
printf 'import reexport;\nvoid main() { float y = lib_fn(1.0); }\n' > "${tree}/pass/through.slang"
t_case "a name reached through __exported import is a use"
t_assert_rc 0 python3 "$gate" "${tree}"

rm -f "${tree}"/pass/*.slang
use dead '// unused_in_comment lib_fn'
t_case "an import named only in a comment is dead"
t_assert_rc 1 python3 "$gate" "${tree}"
python3 "$gate" "${tree}" > "${work}/out" 2>&1
t_assert_grep "${work}/out" '^  pass/dead.slang: import lib is unused'

rm -f "${tree}"/pass/*.slang
printf 'import absent;\nvoid main() {}\n' > "${tree}/pass/orphan.slang"
t_case "an import of no module fails"
t_assert_rc 1 python3 "$gate" "${tree}"

t_summary
