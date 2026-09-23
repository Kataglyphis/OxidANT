#!/usr/bin/env bash
# Fails when any feature set of this workspace could build ort-sys against an
# ONNX Runtime that is not the family's chain build (a pyke download, a
# pkg-config hit, or a link). (1) Cargo.lock: ort-sys has no download deps.
# (2) cargo tree --workspace --all-features --target all: no forbidden ort-sys
# feature, disable-linking on. (3) cargo metadata: EVERY ort declaration in the
# graph carries load-dynamic (every ort-sys one disable-linking), so no subset
# of features can drop it. Does NOT cover which dylib a binary loads at run
# time (crates/inference/src/ort_runtime.rs) or which file packaging ships.
# Usage: scripts/linux/check-ort-chain-only.sh [--root DIR] [--lock-only]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
lock_only=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root) root="${2:?--root needs a directory}"; shift 2 ;;
    --lock-only) lock_only=1; shift ;;
    -h|--help) sed -n '2,10p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done

lock="${root}/Cargo.lock"
if [[ ! -f "${lock}" ]]; then
  printf 'FAIL no Cargo.lock at %s\n' "${lock}" >&2
  exit 1
fi

# The locked dependencies of package $2 in lockfile $1, one name per line.
lock_deps_of() {
  awk -v want="$2" '
    /^\[\[package\]\]/ { name = ""; in_deps = 0; next }
    /^name = / { name = $3; gsub(/"/, "", name); next }
    /^dependencies = \[/ { in_deps = 1; next }
    in_deps && /^\]/ { in_deps = 0; next }
    in_deps && name == want { dep = $1; gsub(/[",]/, "", dep); print dep }
  ' "$1"
}

# Reads `cargo metadata` JSON; prints a FAIL line per ort/ort-sys declaration
# that could link ORT, then "DECLS <count>".
decl_check_py='
import json, sys
need = {"ort": "load-dynamic", "ort-sys": "disable-linking"}
seen = 0
for pkg in json.load(sys.stdin)["packages"]:
    if pkg["name"] == "ort":
        continue  # its ort-sys edge follows the ort features, graded by (2)
    for dep in pkg["dependencies"]:
        want = need.get(dep["name"])
        if want is None:
            continue
        seen += 1
        where = "%s -> %s (%s, %s)" % (pkg["name"], dep["name"],
                                       dep["kind"] or "normal", dep["target"] or "every target")
        if want not in dep["features"]:
            print("FAIL %s does not declare %s" % (where, want))
        if dep["name"] == "ort" and dep["uses_default_features"]:
            print("FAIL %s keeps default features (download-binaries)" % where)
print("DECLS %d" % seen)
'

failures=0
if ! grep -qx 'name = "ort-sys"' "${lock}"; then
  printf 'FAIL Cargo.lock has no ort-sys: nothing left to grade - delete this gate with ORT\n' >&2
  exit 1
fi
ort_sys_deps="$(lock_deps_of "${lock}" ort-sys)"
while read -r dep; do
  case "${dep}" in
    ureq|lzma-rust2|hmac-sha256|pkg-config)
      printf 'FAIL Cargo.lock: ort-sys depends on %s (download-binaries or pkg-config is on)\n' "${dep}" >&2
      failures=$((failures + 1))
      ;;
  esac
done <<< "${ort_sys_deps}"

if [[ "${lock_only}" -eq 0 ]]; then
  if ! tree="$(cd "${root}" && cargo tree --locked -q -e features -i ort-sys \
      --workspace --all-features --target all)"; then
    printf 'FAIL cargo tree could not resolve the ort-sys feature graph\n' >&2
    exit 1
  fi
  for feature in download-binaries pkg-config __tls tls-native tls-native-vendored \
      tls-rustls tls-rustls-no-provider; do
    if grep -qF "ort-sys feature \"${feature}\"" <<< "${tree}"; then
      printf 'FAIL ort-sys feature "%s" is enabled by some feature set:\n' "${feature}" >&2
      grep -F -B0 -A3 "ort-sys feature \"${feature}\"" <<< "${tree}" >&2 || true
      failures=$((failures + 1))
    fi
  done
  if ! grep -qF 'ort-sys feature "disable-linking"' <<< "${tree}"; then
    printf 'FAIL ort-sys feature "disable-linking" is off: ORT would be linked, not runtime-loaded\n' >&2
    failures=$((failures + 1))
  fi

  # (2) sees only the --all-features union; a declaration without load-dynamic
  # links ORT under any feature subset that activates it alone.
  py="$(command -v python3 || command -v python || true)"
  if [[ -z "${py}" ]]; then
    printf 'FAIL no python3 to read cargo metadata with\n' >&2
    exit 1
  fi
  if ! decls="$(cd "${root}" && cargo metadata --locked --format-version 1 \
      | "${py}" -c "${decl_check_py}")"; then
    printf 'FAIL cargo metadata could not list the ort declarations\n' >&2
    exit 1
  fi
  decl_count=0
  while IFS= read -r line; do
    case "${line}" in
      FAIL*) printf '%s\n' "${line}" >&2; failures=$((failures + 1)) ;;
      DECLS*) decl_count="${line#DECLS }" ;;
    esac
  done <<< "${decls}"
  if [[ "${decl_count}" -eq 0 ]]; then
    printf 'FAIL no package declares ort or ort-sys: nothing graded\n' >&2
    failures=$((failures + 1))
  fi
fi

if [[ "${failures}" -gt 0 ]]; then
  printf 'ort chain-only: %d failure(s)\n' "${failures}" >&2
  exit 1
fi
scope="lock + feature graph + ${decl_count:-0} ort declaration(s)"
if [[ "${lock_only}" -eq 1 ]]; then
  scope='lock only'
fi
printf 'ort chain-only OK (%s)\n' "${scope}"
