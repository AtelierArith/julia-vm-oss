import os
import re
import shlex
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET_DIR_HELPER = ROOT / "scripts/cargo_target_dir.sh"
NIGHTLY_WORKFLOW = ROOT / ".github/workflows/nightly-gates.yml"

CONSUMERS = {
    "scripts/test_aot.sh": ("SJULIA_BIN", "JULIARS_BIN"),
    "scripts/metamorphic_equivalence.sh": ("SJULIA_BIN", "JULIARS_BIN"),
    "scripts/aot_numeric_matrix_reduced.sh": ("JULIARS_BIN",),
    "scripts/aot_vm_differential.sh": ("SJULIA_BIN", "JULIARS_BIN"),
    "scripts/aot_fixture_julia_parity.sh": ("JULIARS_BIN",),
    "scripts/aot_fixture_no_silent_mismatch.sh": ("SJULIA_BIN", "JULIARS_BIN"),
    "scripts/aot_cranelift_fixture_differential.sh": ("JULIARS_BIN",),
    "scripts/aot_cranelift_backend_benchmark.sh": ("JULIARS_BIN",),
    "scripts/fixture_julia_parity.sh": ("SJULIA_BIN",),
    "scripts/test_fixture_julia_parity.sh": ("SJULIA_BIN",),
    "scripts/check_fixture_parity_sweep.sh": ("SJULIA_BIN",),
}

# Fail-loud inventory of the exact quoted-variable use sites in each consumer.
# This proves the authoritative variables remain in their reviewed executable,
# check, and forwarding roles; an inert compensating reference cannot preserve
# the inventory. Update a line only after reviewing that shell use site.
QUOTED_BINARY_USE_LINES = {
    "scripts/test_aot.sh": {
        "SJULIA_BIN": [],
        "JULIARS_BIN": ['"$JULIARS_BIN" \\'],
    },
    "scripts/metamorphic_equivalence.sh": {
        "SJULIA_BIN": [
            'if [ -x "$SJULIA_BIN" ]; then',
            '[ -x "$SJULIA_BIN" ] || { fail "build finished but $SJULIA_BIN is still not executable."; return 1; }',
            'run_observed "$modtok" "$SJULIA_BIN" "$src"',
            '"$SJULIA_BIN" "$f")"',
            '"$SJULIA_BIN" "$f" >/dev/null 2>&1; then',
            '"$SJULIA_BIN" "$f")"',
            'generic_obs="$(run_observed \'\' env SJULIA_SSA_PIPELINE=0 "$SJULIA_BIN" "$f")"',
        ],
        "JULIARS_BIN": [
            'if [ -x "$JULIARS_BIN" ]; then',
            '[ -x "$JULIARS_BIN" ] || { fail "build finished but $JULIARS_BIN is still not executable."; return 1; }',
            'if ! timeout 1800 "$JULIARS_BIN" "$fixture" --minimal-prelude -o "$generated_rs" --emit-binary "$aot_bin" >"$compile_out" 2>&1; then',
        ],
    },
    "scripts/aot_numeric_matrix_reduced.sh": {
        "JULIARS_BIN": [
            'if [[ ! -x "$JULIARS_BIN" ]]; then',
            'if ! timeout "$TIMEOUT_SECONDS" "$JULIARS_BIN" --minimal-prelude "$PROBE" -o "$GENERATED_RS" --emit-binary "$AOT_BIN" >"$BUILD_LOG" 2>&1; then',
        ],
    },
    "scripts/aot_vm_differential.sh": {
        "SJULIA_BIN": [
            'if [[ ! -x "$SJULIA_BIN" ]]; then',
            'if ! timeout 120 "$SJULIA_BIN" "$fixture" >"$vm_out" 2>&1; then',
        ],
        "JULIARS_BIN": [
            'if [[ ! -x "$JULIARS_BIN" ]]; then',
            'if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then',
        ],
    },
    "scripts/aot_fixture_julia_parity.sh": {
        "JULIARS_BIN": [
            'if [[ ! -x "$JULIARS_BIN" ]]; then',
            'if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then',
        ],
    },
    "scripts/aot_fixture_no_silent_mismatch.sh": {
        "SJULIA_BIN": [
            'if [[ ! -x "$SJULIA_BIN" ]]; then',
            'if ! timeout 120 "$SJULIA_BIN" "$fixture" >"$vm_stdout" 2>&1; then',
            'if ! timeout 120 "$SJULIA_BIN" "$wrapper" >"$wrapper_vm_stdout" 2>&1; then',
        ],
        "JULIARS_BIN": [
            'if [[ ! -x "$JULIARS_BIN" ]]; then',
            'timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$log" 2>&1',
        ],
    },
    "scripts/aot_cranelift_fixture_differential.sh": {
        "JULIARS_BIN": [
            'if [[ ! -x "$JULIARS_BIN" ]]; then',
            'if ! "$JULIARS_BIN" --help | grep -q -- "--jit-run"; then',
            'if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$rust_bin" >"$tmp_dir/juliars-rust.out" 2>&1; then',
            'if ! timeout 120 "$JULIARS_BIN" "$fixture" --backend cranelift --jit-run >"$cranelift_out" 2>&1; then',
        ],
    },
    "scripts/aot_cranelift_backend_benchmark.sh": {
        "JULIARS_BIN": [
            'if [[ ! -x "$JULIARS_BIN" ]]; then',
            'if ! "$JULIARS_BIN" --help | grep -q -- "--jit-run"; then',
            '"$JULIARS_BIN" "$fixture" --backend rust --check',
            '"$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$rust_bin"',
            '"$JULIARS_BIN" "$fixture" --backend cranelift --check',
            '"$JULIARS_BIN" "$fixture" --backend cranelift --jit-run',
        ],
    },
    "scripts/fixture_julia_parity.sh": {
        "SJULIA_BIN": [
            'if [[ ! -x "$SJULIA_BIN" ]]; then',
            'timeout 120 "$SJULIA_BIN" "$wrapper_path" > "$out_path" 2>&1',
            'if ! timeout 120 "$SJULIA_BIN" "$fixture" > "$sjulia_out" 2>&1; then',
        ],
    },
    "scripts/test_fixture_julia_parity.sh": {
        "SJULIA_BIN": ['if [[ ! -x "$SJULIA_BIN" ]]; then'],
    },
    "scripts/check_fixture_parity_sweep.sh": {
        "SJULIA_BIN": ['if [[ ! -x "$SJULIA_BIN" ]]; then'],
    },
}

TEST_AOT_CARGO_COMMAND_LINES = [
    "timeout 1800 cargo nextest run --locked --release -p subset_julia_vm --features aot \\",
    "timeout 1800 cargo build --locked --release -p subset_julia_vm --features aot --bin juliars",
    "timeout 1800 cargo build --locked --release -p subset_julia_vm --bin sjulia --features repl",
    'timeout 1800 cargo clippy --manifest-path "$tmp_dir/Cargo.toml" -- -D warnings',
]

FIXED_RELEASE_PATH = re.compile(
    r"(?:\$(?:\{(?:ROOT|REPO_ROOT)\}|(?:ROOT|REPO_ROOT))/target"
    r"|\./target|(?<![\w./$])target)"
    r"/release/(?:sjulia|juliars)"
)


def executable_source(path: Path) -> str:
    return "\n".join(
        line
        for line in path.read_text(encoding="utf-8").splitlines()
        if not line.lstrip().startswith("#")
    )


class AotBinaryPathContractTests(unittest.TestCase):
    def test_nightly_fixture_parity_sweep_includes_scope_category(self):
        lines = NIGHTLY_WORKFLOW.read_text(encoding="utf-8").splitlines()
        starts = [
            index
            for index, line in enumerate(lines)
            if "bash scripts/check_fixture_parity_sweep.sh" in line
        ]
        self.assertEqual(len(starts), 1, "expected one nightly sweep command")
        command_lines = []
        index = starts[0]
        while index < len(lines):
            part = lines[index].strip()
            command_lines.append(part.removesuffix("\\").rstrip())
            index += 1
            if not part.endswith("\\"):
                break
        command = " ".join(command_lines)
        tokens = shlex.split(command)
        self.assertIn("scope", tokens, f"nightly fixture parity omits scope: {command}")

    def test_inventory_discovers_every_aot_and_fixture_parity_binary_consumer(self):
        discovered = set()
        candidates = list((ROOT / "scripts").glob("aot_*.sh"))
        candidates.extend((ROOT / "scripts").glob("*fixture*parity*.sh"))
        for candidate in candidates:
            source = executable_source(candidate)
            if re.search(r"(?:sjulia|juliars)(?:_bin|/|\")", source, re.IGNORECASE):
                discovered.add(str(candidate.relative_to(ROOT)))
        self.assertEqual(discovered, set(CONSUMERS) - {
            "scripts/test_aot.sh",
            "scripts/metamorphic_equivalence.sh",
        })

    def test_consumers_derive_release_binaries_from_shared_target_dir_authority(self):
        for relative, binary_variables in CONSUMERS.items():
            with self.subTest(script=relative):
                source = executable_source(ROOT / relative)
                self.assertTrue(
                    "cargo_target_dir.sh" in source,
                    f"{relative} does not source the shared target-dir authority",
                )
                self.assertRegex(
                    source,
                    r'cargo_target_dir="\$\(resolve_cargo_target_dir "\$[^\"]+"\)"',
                    f"{relative} does not resolve cargo_target_dir through the shared authority",
                )
                self.assertIsNone(FIXED_RELEASE_PATH.search(source), relative)
                for variable in binary_variables:
                    binary = variable[:-4].lower()
                    expected = (
                        f'{variable}="${{{variable}:-$cargo_target_dir/release/{binary}}}"'
                    )
                    assignments = [
                        line.strip()
                        for line in source.splitlines()
                        if re.match(rf"^(?:export )?{variable}=", line.strip())
                    ]
                    self.assertEqual(
                        assignments,
                        [expected],
                        f"{relative} must assign {variable} exactly once from the target contract",
                    )
                    actual_use_lines = [
                        line.strip()
                        for line in source.splitlines()
                        if f'"${variable}"' in line
                    ]
                    self.assertEqual(
                        actual_use_lines,
                        QUOTED_BINARY_USE_LINES[relative][variable],
                        f"{relative} must preserve every reviewed {variable} use site",
                    )
                assignment_prefixes = tuple(
                    f"{variable}=" for variable in binary_variables
                )
                non_assignment_source = "\n".join(
                    line
                    for line in source.splitlines()
                    if not line.strip().startswith(assignment_prefixes)
                )
                normalized_paths = re.sub(
                    r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}",
                    r"$\1",
                    non_assignment_source.replace('"', "").replace("'", ""),
                ).replace("/./", "/")
                self.assertNotRegex(
                    normalized_paths,
                    r"\brelease/(?:sjulia|juliars)\b",
                    f"{relative} must not bypass the reviewed binary variables with a direct release path",
                )
                if relative == "scripts/test_aot.sh":
                    cargo_command_lines = [
                        line.strip()
                        for line in source.splitlines()
                        if re.match(r"^\s*timeout [0-9]+ cargo ", line)
                    ]
                    self.assertEqual(
                        cargo_command_lines,
                        TEST_AOT_CARGO_COMMAND_LINES,
                        "scripts/test_aot.sh must preserve each reviewed direct Cargo producer command",
                    )
                    residual_cargo_source = "\n".join(
                        line
                        for line in source.splitlines()
                        if line.strip() not in TEST_AOT_CARGO_COMMAND_LINES
                        and not line.lstrip().startswith("echo ")
                    )
                    self.assertNotRegex(
                        residual_cargo_source,
                        r"\bcargo\s+(?:build|nextest|clippy)\b",
                        "scripts/test_aot.sh must not hide an alternate Cargo producer command",
                    )

    def test_shared_authority_honors_cargo_config_target_directory(self):
        with tempfile.TemporaryDirectory() as temp_dir_name:
            project = Path(temp_dir_name)
            (project / ".cargo").mkdir()
            (project / "src").mkdir()
            (project / "Cargo.toml").write_text(
                '[package]\nname = "target-contract-11695"\nversion = "0.1.0"\n'
                'edition = "2021"\n',
                encoding="utf-8",
            )
            (project / "src/lib.rs").write_text("", encoding="utf-8")
            (project / ".cargo/config.toml").write_text(
                '[build]\ntarget-dir = "configured-target-11695"\n',
                encoding="utf-8",
            )
            env = os.environ.copy()
            env.pop("CARGO_TARGET_DIR", None)
            result = subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; resolve_cargo_target_dir "$2"',
                    "bash",
                    str(TARGET_DIR_HELPER),
                    str(project),
                ],
                env=env,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr or result.stdout)
            self.assertEqual(
                Path(result.stdout.strip()).resolve(),
                (project / "configured-target-11695").resolve(),
            )

    def run_test_aot(self, cargo_target_dir=None, overrides=None):
        overrides = overrides or {}
        with tempfile.TemporaryDirectory() as temp_dir_name:
            temp_dir = Path(temp_dir_name)
            capture = temp_dir / "timeout-env.tsv"
            cargo_capture = temp_dir / "cargo-env.tsv"
            fake_timeout = temp_dir / "timeout"
            fake_timeout.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\t%s\\t%s\\t%s\\n' \"$*\" \"${CARGO_TARGET_DIR-}\" "
                "\"${SJULIA_BIN-}\" \"${JULIARS_BIN-}\" >>\"$SJULIA_TEST_CAPTURE\"\n"
                "shift\n"
                "case \"${1-}\" in\n"
                "  cargo|env) \"$@\" ;;\n"
                "  *) exit 0 ;;\n"
                "esac\n",
                encoding="utf-8",
            )
            fake_timeout.chmod(0o755)
            fake_cargo = temp_dir / "cargo"
            fake_cargo.write_text(
                "#!/usr/bin/env bash\n"
                "if [ \"${1-}\" = metadata ]; then\n"
                "  exit 0\n"
                "fi\n"
                "printf '%s\\t%s\\t%s\\t%s\\n' \"$*\" \"${CARGO_TARGET_DIR-}\" "
                "\"${SJULIA_BIN-}\" \"${JULIARS_BIN-}\" >>\"$SJULIA_CARGO_CAPTURE\"\n"
                "exit 0\n",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)

            env = os.environ.copy()
            env["PATH"] = f"{temp_dir}{os.pathsep}{env['PATH']}"
            env["SJULIA_TEST_CAPTURE"] = str(capture)
            env["SJULIA_CARGO_CAPTURE"] = str(cargo_capture)
            env.pop("CARGO_TARGET_DIR", None)
            env.pop("SJULIA_BIN", None)
            env.pop("JULIARS_BIN", None)
            if cargo_target_dir is not None:
                env["CARGO_TARGET_DIR"] = str(cargo_target_dir)
            env.update(overrides)

            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "scripts/test_aot.sh"),
                    "--no-clippy",
                    "--no-metamorphic",
                ],
                cwd=ROOT,
                env=env,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr or result.stdout)
            rows = [line.split("\t") for line in capture.read_text().splitlines()]
            self.assertGreaterEqual(len(rows), 3)
            cargo_rows = [
                line.split("\t") for line in cargo_capture.read_text().splitlines()
            ]
            self.assertGreaterEqual(len(cargo_rows), 2)
            return [
                (row[-3], row[-2], row[-1]) for row in rows + cargo_rows
            ]

    def assert_all_paths(
        self, rows, expected_target, expected_sjulia, expected_juliars
    ):
        for cargo_target_dir, sjulia, juliars in rows:
            self.assertEqual(cargo_target_dir, str(expected_target))
            self.assertEqual(sjulia, str(expected_sjulia))
            self.assertEqual(juliars, str(expected_juliars))

    def test_test_aot_exports_default_target_binaries(self):
        rows = self.run_test_aot()
        self.assert_all_paths(
            rows,
            ROOT / "target",
            ROOT / "target/release/sjulia",
            ROOT / "target/release/juliars",
        )

    def test_test_aot_exports_external_absolute_target_binaries(self):
        with tempfile.TemporaryDirectory() as external_dir:
            target = Path(external_dir) / "cargo-target"
            rows = self.run_test_aot(target)
            self.assert_all_paths(
                rows,
                target,
                target / "release/sjulia",
                target / "release/juliars",
            )

    def test_test_aot_resolves_relative_target_from_repo_root(self):
        rows = self.run_test_aot("target-relative-11598")
        self.assert_all_paths(
            rows,
            ROOT / "target-relative-11598",
            ROOT / "target-relative-11598/release/sjulia",
            ROOT / "target-relative-11598/release/juliars",
        )

    def test_test_aot_preserves_explicit_binary_overrides(self):
        overrides = {
            "SJULIA_BIN": "/tmp/explicit-sjulia-11598",
            "JULIARS_BIN": "/tmp/explicit-juliars-11598",
        }
        rows = self.run_test_aot("ignored-by-overrides", overrides)
        self.assert_all_paths(
            rows,
            ROOT / "ignored-by-overrides",
            overrides["SJULIA_BIN"],
            overrides["JULIARS_BIN"],
        )


if __name__ == "__main__":
    unittest.main()
