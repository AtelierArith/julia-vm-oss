import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "select_aot_gate.py"
CONFIG = ROOT / ".github" / "aot-gate-paths.txt"


def load_selector():
    spec = importlib.util.spec_from_file_location("select_aot_gate", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class AotGateSelectionTests(unittest.TestCase):
    def setUp(self):
        self.selector = load_selector()
        self.patterns = self.selector.load_patterns(CONFIG)

    def assert_selected(self, path):
        result = self.selector.select_for_paths([path], self.patterns)
        self.assertTrue(result.run_aot, path)
        self.assertTrue(result.matched_patterns, path)

    def assert_not_selected(self, path):
        result = self.selector.select_for_paths([path], self.patterns)
        self.assertFalse(result.run_aot, path)
        self.assertEqual(result.matched_patterns, ())

    def test_shared_inference_core_change_selects_aot(self):
        self.assert_selected(
            "subset_julia_vm_types/src/inference_core/type_core.rs"
        )

    def test_complete_aot_compatibility_path_union_selects_aot(self):
        paths = (
            "subset_julia_vm/src/aot/types.rs",
            "subset_julia_vm/src/bin/aot.rs",
            "subset_julia_vm/src/bin/juliars.rs",
            "subset_julia_vm_runtime/src/lib.rs",
            "subset_julia_vm_types/src/runtime_types/lattice.rs",
            "subset_julia_vm_types/src/inference_cache_key.rs",
            "subset_julia_vm/tests/aot_e2e_tests.rs",
            "subset_julia_vm/tests/core_ir_aot_tests.rs",
            "subset_julia_vm/tests/fixtures/aot/coprime_pi_acceptance_aot.jl",
            "scripts/test_aot.sh",
            "scripts/select_aot_gate.py",
            "scripts/check_aot_gate_selection.sh",
            ".github/aot-gate-paths.txt",
            ".github/workflows/pr-fast.yml",
            ".github/workflows/ci.yml",
            "docs/aot/README.md",
        )
        for path in paths:
            with self.subTest(path=path):
                self.assert_selected(path)

    def test_docs_only_and_unrelated_application_paths_skip_aot(self):
        self.assert_not_selected("docs/vm/TESTING.md")
        self.assert_not_selected("SubsetJuliaVMApp/Views/EditorView.swift")

    def test_github_output_uses_one_boolean_contract(self):
        selection = self.selector.select_for_paths(
            ["subset_julia_vm/src/aot/types.rs"], self.patterns
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "github-output"
            self.selector.write_github_output(output, selection)
            values = dict(
                line.split("=", 1)
                for line in output.read_text(encoding="utf-8").splitlines()
            )
        self.assertEqual(values["aot"], "true")
        self.assertIn("subset_julia_vm/src/aot/**", values["aot_reason"])


if __name__ == "__main__":
    unittest.main()
