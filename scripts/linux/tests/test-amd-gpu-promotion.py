#!/usr/bin/env python3
"""Check AMD promotion wiring; no GitHub service or physical GPU is exercised."""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / ".github/workflows/linux-x64-amd-gpu.yml"
HUB_SHA = "da0bdf709f491b900a9ed02c821d7cbc3a13c8be"


def job(text: str, name: str) -> str:
    match = re.search(
        rf"(?ms)^  {re.escape(name)}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", text
    )
    return match.group(1) if match else ""


LEGACY_STEP = "name: Install, unpack, and run packaged CPU inference"
ISOLATED_STEP = "name: Check immutable packages with isolated CPU inference"
SHARED = (
    "needs.build.outputs.artifact_id",
    "needs.build.outputs.artifact_digest",
    "needs.build.outputs.artifact_files",
)


def step(text: str, name: str) -> str:
    return text.split(f"name: {name}", 1)[-1].split("\n      - ", 1)[0]


def valid_promotion(text: str) -> bool:
    scope = job(text, "scope")
    build = job(text, "build")
    clean = job(text, "clean-host")
    gpu = job(text, "gpu-profile")
    stage = job(text, "stage-verified")
    if LEGACY_STEP not in clean or ISOLATED_STEP not in clean:
        return False
    return (
        trigger_defaults_off(text)
        and build_gates_promotion(scope, build)
        and binds_current_attempt(clean, gpu, stage)
        and clean_host_splits_on_promotion(clean)
        and gpu_profile_wired(gpu, stage)
        and stage_takes_checked_receipts(clean, gpu, stage)
    )


def trigger_defaults_off(text: str) -> bool:
    trigger = text.split("\npermissions:", 1)[0]
    dispatch = trigger.split("  workflow_dispatch:\n", 1)[-1].split(
        "  workflow_call:\n", 1
    )[0]
    return (
        trigger.count(
            "promote-verified:\n        type: boolean\n        default: false"
        )
        == 1
        and re.search(
            r"(?ms)^      promote-verified:\n(?:        [^\n]+\n)*        default: false$",
            dispatch,
        )
        is not None
        and "description: Stage checked AMD packages" in trigger
    )


BUILD_MARKERS = (
    "needs: scope",
    "${{ needs.scope.outputs.suffix }}",
    "name: Require CPU and GPU detector parity for promotion",
    "producer_attempt: ${{ steps.name.outputs.producer_attempt }}",
    "artifact_id: ${{ steps.promotion_upload.outputs.artifact-id }}",
    "artifact_digest: ${{ steps.promotion_upload.outputs.artifact-digest }}",
    "artifact_files: ${{ steps.promotion_files.outputs.files }}",
    "shared/python/verify_locked_files.py",
    "linux/scripts/lib/fetch-data-lock.py",
    "grep -q '^def read_lock('",
    "grep -q '^def matches('",
    "bash scripts/linux/tests/test-check-gpu-detector-parity.sh",
    "bash scripts/linux/tests/test-profile-parity-mutations.sh",
    "third_party/ANTfrastructure/shared/python/compare_detector_parity.py",
    'python3 -I "$parity_tests"',
    "python3 -I \"$parity_comparator\" --help | grep -q -- --pairwise || { echo 'Pinned ANTfrastructure lacks pairwise detector comparison' >&2; exit 1; }",
    "python3 -I \"$ort_profile\" --help | grep -q -- --min-runs || { echo 'Pinned ANTfrastructure lacks per-run ORT profiling' >&2; exit 1; }",
    "python3 scripts/linux/tests/test-detector-parity.py",
)


def build_gates_promotion(scope: str, build: str) -> bool:
    guard = step(build, "Require hardware profiling for promotion")
    parity = step(build, "Require CPU and GPU detector parity for promotion")
    return (
        "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py" in scope
        and "GPU_INVOCATION_SCOPE: ${{ inputs.invocation-scope }}" in scope
        and all(marker in build for marker in BUILD_MARKERS)
        and "        if: ${{ inputs.promote-verified }}" in parity.splitlines()
        and "          exit 1" in parity.splitlines()
        and "continue-on-error" not in parity
        and "if: ${{ inputs.promote-verified && !inputs.verify-gpu }}" in guard
        and "          exit 1" in guard.splitlines()
    )


def binds_current_attempt(clean: str, gpu: str, stage: str) -> bool:
    attempt = (
        "PRODUCER_ATTEMPT: ${{ needs.build.outputs.producer_attempt }}",
        'test "$PRODUCER_ATTEMPT" = "$GITHUB_RUN_ATTEMPT"',
    )
    jobs = (clean, gpu, stage)
    return all(part in section for part in attempt + SHARED for section in jobs)


def clean_host_splits_on_promotion(clean: str) -> bool:
    legacy = clean.split(LEGACY_STEP, 1)[1].split(ISOLATED_STEP, 1)[0]
    isolated = clean.split(ISOLATED_STEP, 1)[1].split(
        "name: Upload the clean-host receipt", 1
    )[0]
    clean_commands = (
        '          deb-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd oxidant',
        '          tarball-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd ./oxidant',
        '          appimage-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd "$APPIMAGE"',
    )
    return (
        "if: ${{ !inputs.promote-verified }}" in legacy
        and "actions/clean-host-package-check@develop" in legacy
        and "if: ${{ inputs.promote-verified }}" in isolated
        and f"actions/clean-host-package-check@{HUB_SHA}" in isolated
        and all(part in isolated for part in SHARED)
        and isolated.count(
            'bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd'
        )
        == 3
        and all(command in isolated.splitlines() for command in clean_commands)
    )


def gpu_profile_wired(gpu: str, stage: str) -> bool:
    profile_line = '          command: env ANTFRASTRUCTURE_DIR="$GITHUB_WORKSPACE/third_party/ANTfrastructure" bash scripts/linux/profile-gpu-packages.sh "$GPU_PACKAGE_DIR" migraphx "$GITHUB_WORKSPACE/crates/inference/tests/fixtures/two_cats.jpg"'
    return (
        "if: ${{ inputs.verify-gpu }}" in gpu
        and "needs: [scope, build, clean-host]" in gpu
        and "    if: ${{ inputs.promote-verified && inputs.verify-gpu }}"
        in stage.splitlines()
        and "needs: [scope, build, clean-host, gpu-profile]" in stage
        and "gpu-vendor: amd" in gpu
        and "gpu-arch: x64" in gpu
        and re.search(
            r"(?m)^      - uses: actions/checkout@[^\n]+\n        with:\n          submodules: true$",
            gpu,
        )
        is not None
        and profile_line in gpu.splitlines()
        and f"actions/gpu-package-check@{HUB_SHA}" in gpu
    )


def stage_takes_checked_receipts(clean: str, gpu: str, stage: str) -> bool:
    clean_name = "unverified-oxidant-amd-clean-receipt-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ needs.scope.outputs.suffix }}"
    gpu_name = "unverified-oxidant-amd-gpu-receipt-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ needs.scope.outputs.suffix }}"
    gpu_file = "oxidant-amd-${{ github.run_id }}-${{ github.run_attempt }}${{ needs.scope.outputs.suffix }}-gpu-receipt.json"
    return (
        f"actions/stage-gpu-artifact@{HUB_SHA}" in stage
        and "skip-decompress: true" in stage
        and "digest-mismatch: error" in stage
        and clean_name in clean
        and clean_name in stage
        and gpu_name in gpu
        and gpu_name in stage
        and gpu.count(gpu_file) == 2
        and gpu_file in stage
        and "verified-oxidant-linux-x64-amd-" in stage
        and "path: ${{ runner.temp }}/oxidant-amd-verified-packages/*" in stage
        and stage.index(f"actions/stage-gpu-artifact@{HUB_SHA}")
        < stage.index("Upload the checked AMD packages")
        and all("always()" not in part for part in (clean, gpu, stage))
    )


class PromotionTest(unittest.TestCase):
    def test_default_off_and_all_checks_bind_current_attempt(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(valid_promotion(text))
        gpu = job(text, "gpu-profile")
        without_submodules = gpu.replace(
            "          submodules: true", "          submodules: false", 1
        )
        self.assertNotEqual(gpu, without_submodules)
        self.assertFalse(valid_promotion(text.replace(gpu, without_submodules, 1)))
        mutations = (
            (
                "oxidant-amd-${{ github.run_id }}-${{ github.run_attempt }}${{ needs.scope.outputs.suffix }}-gpu-receipt.json",
                "oxidant-amd-gpu-receipt.json",
            ),
            (
                "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py",
                "missing-scope-validator.py",
            ),
            ("needs: scope", "needs: []"),
            ("needs.scope.outputs.suffix", "inputs.invocation-scope"),
            (
                "python3 -I \"$parity_comparator\" --help | grep -q -- --pairwise || { echo 'Pinned ANTfrastructure lacks pairwise detector comparison' >&2; exit 1; }",
                "true # skipped pairwise comparison API check",
            ),
            (
                "python3 -I \"$ort_profile\" --help | grep -q -- --min-runs || { echo 'Pinned ANTfrastructure lacks per-run ORT profiling' >&2; exit 1; }",
                "true # skipped per-run ORT API check",
            ),
            (
                "promote-verified:\n        type: boolean\n        default: false",
                "promote-verified:\n        type: boolean\n        default: true",
            ),
            (
                "inputs.promote-verified && !inputs.verify-gpu",
                "inputs.promote-verified && inputs.verify-gpu",
            ),
            (
                "echo '::error::Promotion requires verify-gpu and the hardware profile.'\n          exit 1",
                "echo '::error::Promotion requires verify-gpu and the hardware profile.'\n          true",
            ),
            (
                "name: Require CPU and GPU detector parity for promotion",
                "name: Removed parity gate",
            ),
            (
                "echo '::error::Promotion is disabled until CPU and GPU detector output parity is verified.'\n          exit 1",
                "echo '::error::Promotion is disabled until CPU and GPU detector output parity is verified.'\n          true",
            ),
            (
                "name: Require CPU and GPU detector parity for promotion\n        if: ${{ inputs.promote-verified }}",
                "name: Require CPU and GPU detector parity for promotion\n        if: ${{ !inputs.promote-verified }}",
            ),
            (
                "name: Require CPU and GPU detector parity for promotion\n        if: ${{ inputs.promote-verified }}",
                "name: Require CPU and GPU detector parity for promotion\n        if: ${{ inputs.promote-verified }}\n        continue-on-error: true",
            ),
            (
                "needs: [scope, build, clean-host, gpu-profile]",
                "needs: [scope, build, clean-host]",
            ),
            ("shared/python/verify_locked_files.py", "shared/python/missing.py"),
            ("grep -q '^def read_lock('", "grep -q '^def missing_lock('"),
            ("grep -q '^def matches('", "grep -q '^def missing_matches('"),
            (
                'bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd',
                "unsafe-check",
            ),
            (
                'deb-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd oxidant',
                'deb-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" amd oxidant || true',
            ),
            ('test "$PRODUCER_ATTEMPT" = "$GITHUB_RUN_ATTEMPT"', "true"),
            ("needs.build.outputs.artifact_digest", "untrusted-digest"),
            ("needs.build.outputs.artifact_id", "untrusted-id"),
            ("needs.build.outputs.artifact_files", "untrusted-files"),
            ("gpu-vendor: amd", "gpu-vendor: nvidia"),
            ("gpu-arch: x64", "gpu-arch: arm64"),
            ("digest-mismatch: error", "digest-mismatch: warn"),
            ("skip-decompress: true", "skip-decompress: false"),
            ('"$GPU_PACKAGE_DIR" migraphx', '"$GPU_PACKAGE_DIR" cpu'),
            ('"$GPU_PACKAGE_DIR" migraphx', '"$GPU_PACKAGE_DIR" migraphx || true'),
            (
                f"actions/clean-host-package-check@{HUB_SHA}",
                "actions/clean-host-package-check@develop",
            ),
            (
                f"actions/gpu-package-check@{HUB_SHA}",
                "actions/gpu-package-check@develop",
            ),
            (
                f"actions/stage-gpu-artifact@{HUB_SHA}",
                "actions/stage-gpu-artifact@develop",
            ),
            (
                "if: ${{ inputs.promote-verified && inputs.verify-gpu }}",
                "if: ${{ always() }}",
            ),
        )
        for original, replacement in mutations:
            with self.subTest(original=original):
                self.assertIn(original, text)
                self.assertFalse(valid_promotion(text.replace(original, replacement)))


if __name__ == "__main__":
    unittest.main()
