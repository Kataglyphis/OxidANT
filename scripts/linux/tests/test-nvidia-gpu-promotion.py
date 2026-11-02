#!/usr/bin/env python3
"""Validate NVIDIA workflow wiring without running builds or GPU hardware."""

import ast
import importlib.util
import json
import os
import re
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / ".github/workflows/linux-nvidia-gpu.yml"
BUILD = ROOT / ".github/workflows/linux-nvidia-gpu-build.yml"
HUB_SHA = "da0bdf709f491b900a9ed02c821d7cbc3a13c8be"
SELECTED_BUILDS = (
    "needs.select.result == 'success' && "
    "(needs.select.outputs.architecture == 'arm64' || needs.build-x64.result == 'success') && "
    "(needs.select.outputs.architecture == 'x64' || needs.build-arm64.result == 'success')"
)


def job(text: str, name: str) -> str:
    match = re.search(
        rf"(?ms)^  {re.escape(name)}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", text
    )
    return match.group(1) if match else ""


def evaluate_condition(
    job_text, architecture, results, *, verify=True, promote=True, cancelled=False
):
    """Evaluate the checked workflow condition's supported Boolean expression subset."""
    match = re.search(r"(?m)^    if: \$\{\{ (.+) \}\}$", job_text)
    if match is None:
        raise ValueError("missing job condition")
    expression = match.group(1)
    expression = expression.replace("!cancelled()", repr(not cancelled))
    expression = expression.replace("inputs.verify-gpu", repr(verify))
    expression = expression.replace("inputs.promote-verified", repr(promote))
    expression = expression.replace(
        "needs.select.outputs.architecture", repr(architecture)
    )
    for name, result in results.items():
        expression = expression.replace(f"needs.{name}.result", repr(result))
    expression = expression.replace("&&", " and ").replace("||", " or ")
    tree = ast.parse(expression, mode="eval")
    allowed = (
        ast.Expression,
        ast.BoolOp,
        ast.And,
        ast.Or,
        ast.Compare,
        ast.Eq,
        ast.Constant,
    )
    if any(not isinstance(node, allowed) for node in ast.walk(tree)):
        raise ValueError("unsupported expression")
    return bool(
        eval(compile(tree, "<workflow-condition>", "eval"), {"__builtins__": {}}, {})
    )


def valid(workflow: str, build: str) -> bool:
    trigger = workflow.split("\npermissions:", 1)[0]
    select = job(workflow, "select")
    x64 = job(workflow, "build-x64")
    arm = job(workflow, "build-arm64")
    clean = job(workflow, "clean-host")
    gpu = job(workflow, "gpu-profile")
    stage = job(workflow, "stage-verified")
    producer = job(build, "build")
    guard = producer.split("name: Require hardware profiling for promotion", 1)[
        -1
    ].split("\n      - ", 1)[0]
    parity = producer.split(
        "name: Require CPU and GPU detector parity for promotion", 1
    )[-1].split("\n      - ", 1)[0]
    entries = (
        (x64, "x64", "linux/amd64", "ubuntu-26.04"),
        (arm, "arm64", "linux/arm64", "ubuntu-26.04-arm"),
    )
    for caller, arch, platform, runner in entries:
        if not all(
            part in caller
            for part in (
                "uses: ./.github/workflows/linux-nvidia-gpu-build.yml",
                "needs: select",
                f"if: ${{{{ needs.select.result == 'success' && (needs.select.outputs.architecture == 'both' || needs.select.outputs.architecture == '{arch}') }}}}",
                f"arch: {arch}\n",
                f"platform: {platform}\n",
                f"runner: {runner}\n",
                "invocation-scope: ${{ inputs.invocation-scope }}",
                "${{ needs.select.outputs.suffix }}",
                "verify-gpu: ${{ inputs.verify-gpu }}",
                "promote-verified: ${{ inputs.promote-verified }}",
                "GHCR_PAT: ${{ secrets.GHCR_PAT }}",
            )
        ):
            return False
    if not all(
        "matrix: ${{ fromJSON(needs.select.outputs.matrix) }}" in j
        for j in (clean, gpu, stage)
    ):
        return False
    outputs = ("artifact_id", "artifact_digest", "artifact_files")
    for output in outputs:
        token = f"needs[matrix.producer].outputs.{output}"
        if clean.count(token) != 1:
            return False
        if gpu.count(token) != 1:
            return False
        if stage.count(token) != (2 if output == "artifact_id" else 1):
            return False
        if f"jobs.build.outputs.{output}" not in build:
            return False
    for j in (clean, gpu, stage):
        if (
            not all(
                token in j
                for token in (
                    "PRODUCER_ATTEMPT: ${{ needs[matrix.producer].outputs.producer_attempt }}",
                    'test "$PRODUCER_ATTEMPT" = "$GITHUB_RUN_ATTEMPT"',
                )
            )
            or "always()" in j
        ):
            return False
    clean_receipt = "unverified-oxidant-nvidia-${{ matrix.arch }}-clean-receipt-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ needs.select.outputs.suffix }}"
    gpu_receipt = "unverified-oxidant-nvidia-${{ matrix.arch }}-gpu-receipt-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ needs.select.outputs.suffix }}"
    gpu_file = "oxidant-nvidia-${{ github.run_id }}-${{ github.run_attempt }}-${{ matrix.arch }}${{ needs.select.outputs.suffix }}-gpu-receipt.json"
    input_name = "oxidant-linux-${{ inputs.arch }}-nvidia-host-runtime-candidate-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ steps.scope.outputs.suffix }}"
    consumer_name = "oxidant-linux-${{ matrix.arch }}-nvidia-host-runtime-candidate-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ needs.select.outputs.suffix }}"
    checks = (
        gpu.count(gpu_file) == 2 and gpu_file in stage,
        trigger.count(
            "promote-verified:\n        type: boolean\n        default: false"
        )
        == 1,
        re.search(
            r"(?ms)^      promote-verified:\n(?:        [^\n]+\n)*        default: false$",
            trigger,
        )
        is not None,
        "      architecture:\n        type: string\n        default: both" in trigger,
        "options: [both, x64, arm64]\n        default: both" in trigger,
        "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py" in select,
        "GPU_INVOCATION_SCOPE: ${{ inputs.invocation-scope }}" in select,
        "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py" in producer,
        "GPU_INVOCATION_SCOPE: ${{ inputs.invocation-scope }}" in producer,
        producer.count(input_name) == 2,
        clean.count(consumer_name) == 3 and gpu.count(consumer_name) == 1,
        "verified-oxidant-linux-${{ matrix.arch }}-nvidia-${{ github.run_id }}-attempt-${{ github.run_attempt }}${{ needs.select.outputs.suffix }}"
        in stage,
        '"arch":"x64","producer":"build-x64","runner":"ubuntu-26.04","runner-arch":"X64"'
        in select,
        '"arch":"arm64","producer":"build-arm64","runner":"ubuntu-26.04-arm","runner-arch":"ARM64"'
        in select,
        "*) echo '::error::architecture must be both, x64 or arm64'; exit 1" in select,
        "permissions:\n  contents: read" in workflow
        and "permissions:\n  contents: read" in build,
        "group: linux-nvidia-gpu-${{ github.ref }}-${{ github.workflow }}" in workflow,
        "name: Require CPU and GPU detector parity for promotion" in producer,
        "        if: ${{ inputs.promote-verified }}" in parity.splitlines(),
        "          exit 1" in parity.splitlines(),
        "continue-on-error" not in parity,
        "if: ${{ inputs.promote-verified && !inputs.verify-gpu }}" in guard,
        "          exit 1" in guard.splitlines(),
        "x64:linux/amd64:ubuntu-26.04|arm64:linux/arm64:ubuntu-26.04-arm" in producer,
        "*) echo '::error::Unsupported NVIDIA build tuple'; exit 1" in producer,
        "producer_attempt: ${{ steps.name.outputs.producer_attempt }}" in producer,
        "artifact_id: ${{ steps.promotion_upload.outputs.artifact-id }}" in producer,
        "artifact_digest: ${{ steps.promotion_upload.outputs.artifact-digest }}"
        in producer,
        "artifact_files: ${{ steps.promotion_files.outputs.files }}" in producer,
        "if: ${{ !inputs.promote-verified }}" in producer
        and "path: dist/**" in producer,
        "if: ${{ inputs.promote-verified }}" in producer
        and producer.count("overwrite: false") == 2,
        "shared/python/verify_locked_files.py" in producer,
        "third_party/ANTfrastructure/shared/python/compare_detector_parity.py"
        in producer,
        'python3 -I "$parity_tests"' in producer,
        "python3 -I \"$parity_comparator\" --help | grep -q -- --pairwise || { echo 'Pinned ANTfrastructure lacks pairwise detector comparison' >&2; exit 1; }"
        in producer,
        "python3 -I \"$ort_profile\" --help | grep -q -- --min-runs || { echo 'Pinned ANTfrastructure lacks per-run ORT profiling' >&2; exit 1; }"
        in producer,
        "bash scripts/linux/tests/test-check-gpu-detector-parity.sh" in producer,
        "bash scripts/linux/tests/test-profile-parity-mutations.sh" in producer,
        "python3 scripts/linux/tests/test-detector-parity.py" in producer,
        "if: ${{ !inputs.promote-verified }}" in clean,
        "actions/clean-host-package-check@develop" in clean,
        f"actions/clean-host-package-check@{HUB_SHA}" in clean,
        clean.count(
            'bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" nvidia'
        )
        == 3,
        all(
            line in clean.splitlines()
            for line in (
                '          deb-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" nvidia oxidant',
                '          tarball-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" nvidia ./oxidant',
                '          appimage-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" nvidia "$APPIMAGE"',
            )
        ),
        "needs: [select, build-x64, build-arm64]" in clean,
        f"if: ${{{{ !cancelled() && {SELECTED_BUILDS} }}}}" in clean,
        "needs: [select, build-x64, build-arm64, clean-host]" in gpu,
        f"if: ${{{{ !cancelled() && inputs.verify-gpu && {SELECTED_BUILDS} && needs.clean-host.result == 'success' }}}}"
        in gpu,
        f"actions/gpu-package-check@{HUB_SHA}" in gpu,
        re.search(
            r"(?m)^      - uses: actions/checkout@[^\n]+\n        with:\n          submodules: true$",
            gpu,
        )
        is not None,
        '          command: env ANTFRASTRUCTURE_DIR="$GITHUB_WORKSPACE/third_party/ANTfrastructure" bash scripts/linux/profile-gpu-packages.sh "$GPU_PACKAGE_DIR" cuda "$GITHUB_WORKSPACE/crates/inference/tests/fixtures/two_cats.jpg"'
        in gpu.splitlines(),
        "needs: [select, build-x64, build-arm64, clean-host, gpu-profile]" in stage,
        f"if: ${{{{ !cancelled() && inputs.promote-verified && inputs.verify-gpu && {SELECTED_BUILDS} && needs.clean-host.result == 'success' && needs.gpu-profile.result == 'success' }}}}"
        in stage,
        f"actions/stage-gpu-artifact@{HUB_SHA}" in stage,
        "skip-decompress: true" in stage and stage.count("digest-mismatch: error") == 3,
        clean_receipt in clean and clean_receipt in stage,
        gpu_receipt in gpu and gpu_receipt in stage,
        all("gpu-arch: ${{ matrix.arch }}" in j for j in (clean, gpu, stage)),
        all("gpu-vendor: nvidia" in j for j in (clean, gpu, stage)),
        "verified-oxidant-linux-${{ matrix.arch }}-nvidia-" in stage,
        0
        <= stage.find(f"actions/stage-gpu-artifact@{HUB_SHA}")
        < stage.find("Upload the checked NVIDIA packages"),
        "release" not in stage.lower(),
    )
    return all(checks)


class NvidiaPromotionTests(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "the selector runs in Linux Bash")
    def test_actual_selector_accepts_only_supported_architectures(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        selector = job(workflow, "select")
        script = textwrap.dedent(selector.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "output"
            for request, expected in (
                ("both", ["x64", "arm64"]),
                ("x64", ["x64"]),
                ("arm64", ["arm64"]),
                ("", None),
                ("riscv64", None),
                ("x64; touch injected", None),
            ):
                with self.subTest(request=request):
                    output.unlink(missing_ok=True)
                    result = subprocess.run(
                        ["bash", "-euo", "pipefail", "-c", script],
                        env={
                            **os.environ,
                            "ARCHITECTURE": request,
                            "GITHUB_OUTPUT": str(output),
                        },
                        cwd=tmp,
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    if expected is None:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertFalse(output.exists())
                        continue
                    self.assertEqual(result.returncode, 0, result.stderr)
                    lines = dict(
                        line.split("=", 1) for line in output.read_text().splitlines()
                    )
                    rows = json.loads(lines["matrix"])["include"]
                    self.assertEqual(lines["architecture"], request)
                    self.assertEqual([row["arch"] for row in rows], expected)
                    for row in rows:
                        self.assertEqual(row["producer"], "build-" + row["arch"])
                    self.assertFalse((Path(tmp) / "injected").exists())

    def test_selected_producers_control_all_downstream_jobs(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        jobs = {
            name: job(workflow, name)
            for name in ("clean-host", "gpu-profile", "stage-verified")
        }
        for arch, x64, arm in (
            ("x64", "success", "skipped"),
            ("arm64", "skipped", "success"),
            ("both", "success", "success"),
        ):
            base = {
                "select": "success",
                "build-x64": x64,
                "build-arm64": arm,
                "clean-host": "success",
                "gpu-profile": "success",
            }
            for producer in ("build-x64", "build-arm64"):
                selected = arch == "both" or producer == "build-" + arch
                self.assertEqual(
                    evaluate_condition(job(workflow, producer), arch, base), selected
                )
                for failed_selector in ("failure", "cancelled", "skipped"):
                    self.assertFalse(
                        evaluate_condition(
                            job(workflow, producer),
                            arch,
                            {**base, "select": failed_selector},
                        )
                    )
            for name, text in jobs.items():
                with self.subTest(arch=arch, job=name):
                    self.assertTrue(evaluate_condition(text, arch, base))
                    self.assertFalse(
                        evaluate_condition(text, arch, base, cancelled=True)
                    )
                    for chosen in (
                        ["build-x64"]
                        if arch == "x64"
                        else ["build-arm64"]
                        if arch == "arm64"
                        else ["build-x64", "build-arm64"]
                    ):
                        for failure in ("failure", "cancelled", "skipped"):
                            self.assertFalse(
                                evaluate_condition(
                                    text, arch, {**base, chosen: failure}
                                )
                            )
                    for status in ("failure", "cancelled", "skipped"):
                        self.assertFalse(
                            evaluate_condition(text, arch, {**base, "select": status})
                        )
                    if name != "clean-host":
                        for status in ("failure", "cancelled", "skipped"):
                            self.assertFalse(
                                evaluate_condition(
                                    text, arch, {**base, "clean-host": status}
                                )
                            )
                        self.assertFalse(
                            evaluate_condition(text, arch, base, verify=False)
                        )
                    if name == "stage-verified":
                        for status in ("failure", "cancelled", "skipped"):
                            self.assertFalse(
                                evaluate_condition(
                                    text, arch, {**base, "gpu-profile": status}
                                )
                            )
                        self.assertFalse(
                            evaluate_condition(text, arch, base, promote=False)
                        )

    def test_wiring_and_mutations(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        build = BUILD.read_text(encoding="utf-8")
        self.assertTrue(valid(workflow, build))
        gpu = job(workflow, "gpu-profile")
        without_submodules = gpu.replace(
            "          submodules: true", "          submodules: false", 1
        )
        self.assertNotEqual(gpu, without_submodules)
        self.assertFalse(valid(workflow.replace(gpu, without_submodules, 1), build))
        mutations = (
            (
                "oxidant-nvidia-${{ github.run_id }}-${{ github.run_attempt }}-${{ matrix.arch }}${{ needs.select.outputs.suffix }}-gpu-receipt.json",
                "oxidant-nvidia-gpu-receipt.json",
            ),
            (
                "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py",
                "missing-scope-validator.py",
            ),
            ("needs.select.outputs.suffix", "inputs.invocation-scope"),
            ('"producer":"build-x64"', '"producer":"build-arm64"'),
            ('"producer":"build-arm64"', '"producer":"build-x64"'),
            ("needs.select.result == 'success'", "true"),
            ("needs.build-x64.result == 'success'", "true"),
            ("needs.build-arm64.result == 'success'", "true"),
            ("needs.clean-host.result == 'success'", "true"),
            ("needs.gpu-profile.result == 'success'", "true"),
            ("!cancelled()", "true"),
            (
                "needs.select.outputs.architecture == 'both' || needs.select.outputs.architecture == 'x64'",
                "true",
            ),
            ("needs[matrix.producer].outputs.artifact_id", "untrusted-id"),
            ("needs[matrix.producer].outputs.artifact_digest", "untrusted-digest"),
            ("needs[matrix.producer].outputs.artifact_files", "untrusted-files"),
            ('test "$PRODUCER_ATTEMPT" = "$GITHUB_RUN_ATTEMPT"', "true"),
            ("gpu-arch: ${{ matrix.arch }}", "gpu-arch: x64"),
            ("gpu-vendor: nvidia", "gpu-vendor: amd"),
            ("digest-mismatch: error", "digest-mismatch: warn"),
            ("skip-decompress: true", "skip-decompress: false"),
            ('"$GPU_PACKAGE_DIR" cuda', '"$GPU_PACKAGE_DIR" cpu'),
            ('"$GPU_PACKAGE_DIR" cuda', '"$GPU_PACKAGE_DIR" cuda || true'),
            (
                'deb-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" nvidia oxidant',
                'deb-command: bash "$GITHUB_WORKSPACE/scripts/linux/check-gpu-clean-host.sh" nvidia oxidant || true',
            ),
            ('"runner-arch":"ARM64"', '"runner-arch":"X64"'),
            (
                "needs: [select, build-x64, build-arm64, clean-host, gpu-profile]",
                "needs: [select, build-x64, build-arm64, clean-host]",
            ),
            ("GHCR_PAT: ${{ secrets.GHCR_PAT }}", "GHCR_PAT: ''"),
            (
                f"actions/stage-gpu-artifact@{HUB_SHA}",
                "actions/stage-gpu-artifact@develop",
            ),
        )
        for original, replacement in mutations:
            with self.subTest(original=original):
                self.assertIn(original, workflow)
                self.assertFalse(
                    valid(workflow.replace(original, replacement, 1), build)
                )
        for original, replacement in (
            (
                "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py",
                "missing-scope-validator.py",
            ),
            ("steps.scope.outputs.suffix", "inputs.invocation-scope"),
            (
                "python3 -I \"$parity_comparator\" --help | grep -q -- --pairwise || { echo 'Pinned ANTfrastructure lacks pairwise detector comparison' >&2; exit 1; }",
                "true # skipped pairwise comparison API check",
            ),
            (
                "python3 -I \"$ort_profile\" --help | grep -q -- --min-runs || { echo 'Pinned ANTfrastructure lacks per-run ORT profiling' >&2; exit 1; }",
                "true # skipped per-run ORT API check",
            ),
            (
                "steps.promotion_upload.outputs.artifact-id",
                "steps.other.outputs.artifact-id",
            ),
            (
                "steps.promotion_upload.outputs.artifact-digest",
                "steps.other.outputs.artifact-digest",
            ),
            ("steps.promotion_files.outputs.files", "steps.other.outputs.files"),
            (
                "inputs.promote-verified && !inputs.verify-gpu",
                "inputs.promote-verified && inputs.verify-gpu",
            ),
            (
                "echo '::error::Promotion requires verify-gpu and the hardware profile.'\n          exit 1",
                "echo '::error::Promotion requires verify-gpu and the hardware profile.'\n          true",
            ),
            (
                "arm64:linux/arm64:ubuntu-26.04-arm",
                "arm64:linux/amd64:ubuntu-26.04-arm",
            ),
            ("shared/python/verify_locked_files.py", "shared/python/missing.py"),
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
        ):
            with self.subTest(original=original):
                self.assertIn(original, build)
                self.assertFalse(valid(workflow, build.replace(original, replacement)))

        for original in (
            "needs[matrix.producer].outputs.artifact_id",
            "needs[matrix.producer].outputs.artifact_digest",
            "needs[matrix.producer].outputs.artifact_files",
            'test "$PRODUCER_ATTEMPT" = "$GITHUB_RUN_ATTEMPT"',
            "gpu-arch: ${{ matrix.arch }}",
            "gpu-vendor: nvidia",
        ):
            parts = workflow.split(original)
            self.assertGreaterEqual(len(parts), 4, original)
            for index in range(len(parts) - 1):
                with self.subTest(original=original, occurrence=index):
                    mutant = (
                        original.join(parts[: index + 1])
                        + "invalid"
                        + original.join(parts[index + 1 :])
                    )
                    self.assertFalse(valid(mutant, build))
        for guard in (
            "needs.select.result == 'success'",
            "needs.build-x64.result == 'success'",
            "needs.build-arm64.result == 'success'",
            "needs.clean-host.result == 'success'",
            "needs.gpu-profile.result == 'success'",
            "!cancelled()",
        ):
            parts = workflow.split(guard)
            self.assertGreaterEqual(len(parts), 2, guard)
            for index in range(len(parts) - 1):
                with self.subTest(guard=guard, occurrence=index):
                    mutant = (
                        guard.join(parts[: index + 1])
                        + "true"
                        + guard.join(parts[index + 1 :])
                    )
                    self.assertFalse(valid(mutant, build))

    def test_attempt_receipts_are_unique_per_architecture(self) -> None:
        for kind in ("clean", "gpu"):
            names = {
                f"unverified-oxidant-nvidia-{arch}-{kind}-receipt-42-attempt-{attempt}"
                for arch in ("x64", "arm64")
                for attempt in (1, 2)
            }
            self.assertEqual(len(names), 4)
            self.assertNotIn(
                f"unverified-oxidant-nvidia-arm64-{kind}-receipt-42-attempt-2",
                {
                    f"unverified-oxidant-nvidia-{arch}-{kind}-receipt-42-attempt-1"
                    for arch in ("x64", "arm64")
                },
            )

    @unittest.skipIf(os.name == "nt", "the staging action needs Linux renameat2")
    def test_both_architectures_stage_and_reject_wrong_identity(self) -> None:
        hub = Path(
            os.environ.get("ANTFRASTRUCTURE_DIR", ROOT / "third_party/ANTfrastructure")
        )
        fixture_path = hub / "shared/python/tests/test_stage_gpu_artifact.py"
        spec = importlib.util.spec_from_file_location("hub_stage_fixture", fixture_path)
        if spec is None or spec.loader is None:
            self.fail(f"cannot load {fixture_path}")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        identities = []
        for arch, artifact_id in (("x64", "31"), ("arm64", "32")):
            with self.subTest(arch=arch):
                fixture = module.StageTest(
                    "test_matching_receipts_and_archive_stage_exact_bytes"
                )
                fixture.setUp()
                try:
                    fixture.data["engine.tar.gz"] = f"{arch} tar payload".encode()
                    (fixture.files / "engine.tar.gz").write_bytes(
                        fixture.data["engine.tar.gz"]
                    )
                    fixture.identity.update({"arch": arch, "artifact-id": artifact_id})
                    fixture.make_archive()
                    fixture.make_receipts()
                    identities.append(
                        (artifact_id, fixture.identity["artifact-digest"])
                    )
                    for receipt in (fixture.clean, fixture.gpu):
                        original = receipt.read_text()
                        for key, value in (
                            ("artifact_id", "99"),
                            ("artifact_digest", "f" * 64),
                            ("run_attempt", "1"),
                            ("arch", "arm64" if arch == "x64" else "x64"),
                        ):
                            changed = json.loads(original)
                            changed["identity"][key] = value
                            receipt.write_text(json.dumps(changed))
                            fixture.rejected()
                            del changed["identity"][key]
                            receipt.write_text(json.dumps(changed))
                            fixture.rejected()
                        for filename in fixture.data:
                            changed = json.loads(original)
                            changed["files"][filename] = "f" * 64
                            receipt.write_text(json.dumps(changed))
                            fixture.rejected()
                        receipt.unlink()
                        fixture.rejected()
                        receipt.write_text(original)
                    fixture.rejected("--run-attempt", "3")
                    fixture.test_matching_receipts_and_archive_stage_exact_bytes()
                finally:
                    fixture.doCleanups()
        self.assertNotEqual(identities[0][0], identities[1][0])
        self.assertNotEqual(identities[0][1], identities[1][1])


if __name__ == "__main__":
    unittest.main()
