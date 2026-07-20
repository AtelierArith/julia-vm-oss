import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "select_pr_fixture_tests.py"
CONFIG = ROOT / ".github" / "fixture-selection.toml"


def load_selector():
    spec = importlib.util.spec_from_file_location("select_pr_fixture_tests", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PrFixtureSelectionTests(unittest.TestCase):
    def test_base_strings_change_selects_smoke_and_string_categories(self):
        selector = load_selector()
        config = selector.load_config(CONFIG)

        result = selector.select_for_paths(
            ["subset_julia_vm/src/julia/base/strings/basic.jl"], config
        )

        self.assertEqual(result.scope, "selected")
        self.assertEqual(
            result.filters,
            (
                "essentials::",
                "arithmetic::",
                "strings::",
                "string_::",
            ),
        )
        self.assertIn("base-strings", result.reasons)

    def test_changed_fixture_selects_own_category_plus_smoke(self):
        selector = load_selector()
        config = selector.load_config(CONFIG)

        result = selector.select_for_paths(
            ["subset_julia_vm/tests/fixtures/dict/dict_iteration.jl"], config
        )

        self.assertEqual(result.scope, "selected")
        self.assertEqual(result.filters, ("essentials::", "arithmetic::", "dict::"))
        self.assertIn("changed-fixture:dict", result.reasons)

    def test_unknown_rust_path_falls_back_to_full_suite(self):
        selector = load_selector()
        config = selector.load_config(CONFIG)

        result = selector.select_for_paths(
            ["subset_julia_vm_vm/src/vm/new_shared_runtime_path.rs"], config
        )

        self.assertEqual(result.scope, "all")
        self.assertEqual(result.filters, ())
        self.assertIn("unmapped-rust", result.reasons)

    def test_docs_only_selects_no_fixture_run(self):
        selector = load_selector()
        config = selector.load_config(CONFIG)

        result = selector.select_for_paths(["docs/vm/TESTING.md"], config)

        self.assertEqual(result.scope, "none")
        self.assertEqual(result.filters, ())
        self.assertEqual(result.reasons, ("docs-only",))


if __name__ == "__main__":
    unittest.main()
