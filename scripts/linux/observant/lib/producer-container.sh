#!/usr/bin/env bash

_observant_producer_run() {
  local action="$1" image="$2" target_volume="$3" shell_mode="$4"
  shift 4
  local -a entrypoint=() shell=(bash)
  case "${shell_mode}" in
    image) ;;
    bash) entrypoint=(--entrypoint bash); shell=() ;;
    *) printf 'unknown producer shell mode: %s\n' "${shell_mode}" >&2; return 2 ;;
  esac

  case "${action}" in
    present)
      local producer="$1"
      shift
      nerdctl run "$@" --rm --user 0:0 "${entrypoint[@]}" \
        -v "${target_volume}":/cargo-target "${image}" "${shell[@]}" -c \
        "test -x \"\$1\"" observant-probe "${producer}" >/dev/null 2>&1
      ;;
    build)
      local workspace="$1" cargo_volume="$2" source_dir="$3"
      shift 3
      nerdctl run "$@" --rm --user 0:0 --network host \
        -v "${workspace}":/workspace \
        -v "${target_volume}":/cargo-target \
        -v "${cargo_volume}":/cargo-home \
        -e CARGO_TARGET_DIR=/cargo-target -e CARGO_HOME=/cargo-home \
        "${entrypoint[@]}" "${image}" "${shell[@]}" -lc \
        "cd \"\$1\" && cargo build --release --locked -p kataglyphis_observant" \
        observant-build "${source_dir}"
      ;;
    *) printf 'unknown producer action: %s\n' "${action}" >&2; return 2 ;;
  esac
}

observant_producer_present() {
  local image="$1" target_volume="$2" producer="$3" shell_mode="$4"
  shift 4
  _observant_producer_run present "${image}" "${target_volume}" "${shell_mode}" "${producer}" "$@"
}

observant_build_producer() {
  local image="$1" workspace="$2" target_volume="$3" cargo_volume="$4"
  local source_dir="$5" shell_mode="$6"
  shift 6
  _observant_producer_run build "${image}" "${target_volume}" "${shell_mode}" \
    "${workspace}" "${cargo_volume}" "${source_dir}" "$@"
}
