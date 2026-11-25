#!/usr/bin/env bash
# generate-shader-vectors.py on a copy of the tree: --check holds the committed files, and a case an f32 kernel cannot meet stops it.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
repo="${tests_dir}/../../.."
mkdir -p "${work}/scripts" "${work}/shaders/tests/vectors" "${work}/shaders/common"

# fresh: the copy reset to this checkout's generator and its modules, model, energy fit and vector files.
fresh() {
    cp "${repo}/scripts/generate-shader-vectors.py" "${repo}/scripts/shader_model.py" \
        "${repo}/scripts/shader_model_lighting.py" "${repo}/scripts/shader_model_path.py" \
        "${repo}/scripts/shader_vector_format.py" "${repo}/scripts/shader_vectors_lighting.py" \
        "${repo}/scripts/shader_vectors_path.py" "${work}/scripts/"
    cp "${repo}/shaders/common/brdf_energy_fit.slang" "${work}/shaders/common/"
    cp "${repo}"/shaders/tests/vectors/*.json "${work}/shaders/tests/vectors/"
}
gen="${work}/scripts/generate-shader-vectors.py"

fresh
t_case "the committed vector files match a fresh run"
t_assert_rc 0 python3 "${gen}" --check

fresh
sed -i 's/"lambert_diffuse": \[0.254647905655,/"lambert_diffuse": [0.2547,/' "${work}/shaders/tests/vectors/brdf.json"
t_case "an edited reference fails the check by file"
suite_run python3 "${gen}" --check
t_assert_eq 1 "$(cat "${work}/rc")"
t_assert_grep "${work}/out" '^  shaders/tests/vectors/brdf.json$'

fresh
sed -i 's/0.0, 1.0), 5.0)/0.0, 1.0), 5.5)/' "${work}/scripts/shader_model.py"
t_case "a changed model fails the check for every module it reaches"
suite_run python3 "${gen}" --check
t_assert_grep "${work}/out" '^  shaders/tests/vectors/brdf.json$'
t_assert_grep "${work}/out" '^  shaders/tests/vectors/path_estimator.json$'

fresh
sed -i 's/float2(0.8300566, 0.0006474)/float2(0.8400566, 0.0006474)/' "${work}/shaders/common/brdf_energy_fit.slang"
t_case "a refitted energy spline fails the check for every module whose brdf reads it"
suite_run python3 "${gen}" --check
t_assert_grep "${work}/out" '^  shaders/tests/vectors/brdf.json$'
t_assert_grep "${work}/out" '^  shaders/tests/vectors/path_estimator.json$'

fresh
sed -i 's/("roughness zero evaluates as the floor too", {"roughness": \[0.0\], "n_dot_h": \[0.99\]})/("sharp", {"roughness": [0.0], "l": unit(0.05, 1.0, 0.0)})/' "${gen}"
t_case "a case whose f32 evaluation strays past its tolerance stops the generator"
suite_run python3 "${gen}" --check
t_assert_eq 1 "$(cat "${work}/rc")"
t_assert_grep "${work}/out" "case 'sharp' is too close to the f32 limits"

fresh
sed -i '/("light below the horizon contributes nothing"/d' "${gen}"
t_case "a branch no case reaches stops the generator"
suite_run python3 "${gen}" --check
t_assert_grep "${work}/out" "no case reaches \['light below the horizon'\]"

t_summary
