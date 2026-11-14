#!/usr/bin/env bash
# check-genai-signatures.py over a fixture header and bindings: a changed or missing signature fails and is shown.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
check="${tests_dir}/../check-genai-signatures.py"

cat > "${work}/genai.rs" <<'RS'
create_config: bind!(library, "OgaCreateConfig"),
create_buffer_config: bind!(
    library,
    "OgaCreateConfigFromBuffer"
),
set_guidance: lookup!(library, "OgaGeneratorParamsSetGuidance"),
RS
cat > "${work}/ort_genai_c.h" <<'H'
/* OGA_EXPORT OgaResult* OGA_API_CALL OgaCreateConfig(int commented_out); */
OGA_EXPORT OgaResult* OGA_API_CALL OgaCreateConfig(const char* config_path, OgaConfig** out);
OGA_EXPORT OgaResult* OGA_API_CALL OgaCreateConfigFromBuffer(const char* config_json,
                                                              OgaConfig** out);  // a declaration over two lines
OGA_EXPORT OgaResult* OGA_API_CALL OgaGeneratorParamsSetGuidance(OgaGeneratorParams* params, const char* type,
                                                                  const char* data, bool enable_ff_tokens);
OGA_EXPORT void OGA_API_CALL OgaUnbound(void);
H
run() { suite_run python3 "${check}" --header "${work}/ort_genai_c.h" --bindings "${work}/genai.rs" --snapshot "${work}/snapshot.txt" "$@"; }

run --write
t_case "the snapshot holds each bound symbol once, on one line, the optional lookup included, nothing unbound"
t_assert_grep "${work}/rc" '^0$'
t_assert_eq 3 "$(wc -l < "${work}/snapshot.txt" | tr -d ' ')" "three bound symbols"
t_assert_grep "${work}/snapshot.txt" '^OgaCreateConfig: OgaResult\* \(const char\* config_path, OgaConfig\*\* out\)$'
t_assert_grep "${work}/snapshot.txt" '^OgaCreateConfigFromBuffer: OgaResult\* \(const char\* config_json, OgaConfig\*\* out\)$'
t_assert_grep "${work}/snapshot.txt" '^OgaGeneratorParamsSetGuidance: '
t_assert_no_grep "${work}/snapshot.txt" 'OgaUnbound|commented_out'

run
t_case "an unchanged header passes"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" 'OK, 3 bound GenAI signatures match'

sed -i 's/const char\* data, bool enable_ff_tokens/const char* data/' "${work}/ort_genai_c.h"
run
t_case "a changed signature fails and shows the old and the new line"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" '^-OgaGeneratorParamsSetGuidance: .*bool enable_ff_tokens\)$'
t_assert_grep "${work}/out" '^\+OgaGeneratorParamsSetGuidance: .*const char\* data\)$'

sed -i '/OgaCreateConfig(const char/d' "${work}/ort_genai_c.h"
run
t_case "a bound symbol the header no longer declares fails by name"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" '^\+OgaCreateConfig: NOT DECLARED in ort_genai_c.h$'

suite_run python3 "${check}" --header "${work}/absent.h" --bindings "${work}/genai.rs" --snapshot "${work}/snapshot.txt"
t_case "no header is a usage error, not a pass"
t_assert_grep "${work}/rc" '^2$'

t_summary
