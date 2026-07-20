#!/usr/bin/env python3

import tempfile
import unittest
from pathlib import Path

from semantic_id_inventory import (
    IDENTITY_BEARING,
    INERT,
    LEXICAL_BOUNDARY,
    classify_effective_domain,
    classify_verdict,
    run_check_name_based_lookup_live_counts,
    write_tsv,
)


class VerdictClassificationTests(unittest.TestCase):
    def test_classified_typevar_boundaries_are_lexical(self) -> None:
        boundaries = (
            (
                "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs",
                "LexicalTypeBindings",
            ),
            (
                "subset_julia_vm_types/src/inference_core/type_core.rs",
                "RenderedTypeParseCache",
            ),
        )
        for path, symbol in boundaries:
            with self.subTest(path=path, symbol=symbol):
                self.assertEqual(
                    classify_verdict("map_decl", "typevar", path, symbol),
                    LEXICAL_BOUNDARY,
                )

    def test_live_unclassified_typevar_core_binding_count_is_zero(self) -> None:
        self.assertEqual(
            run_check_name_based_lookup_live_counts()["typevar_core_bindings"],
            0,
        )

    def test_classified_typevar_boundaries_override_mechanical_struct_domain(self) -> None:
        self.assertEqual(
            classify_effective_domain(
                "struct",
                "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs",
                "LexicalTypeBindings",
            ),
            "typevar",
        )
        self.assertEqual(
            classify_effective_domain(
                "function",
                "subset_julia_vm_compile/src/compile/context.rs",
                "function_indices",
            ),
            "function",
        )

    def test_unreviewed_core_domain_defaults_to_identity_bearing(self) -> None:
        self.assertEqual(
            classify_verdict(
                "map_decl",
                "function",
                "subset_julia_vm/src/compile/context.rs",
                "function_indices",
            ),
            IDENTITY_BEARING,
        )
        self.assertEqual(
            classify_verdict(
                "anchor",
                "struct",
                "subset_julia_vm/src/compile/type_helpers.rs",
                "struct_table_bare_gets_compile",
            ),
            IDENTITY_BEARING,
        )

    def test_unreviewed_other_domain_also_fails_closed(self) -> None:
        self.assertEqual(
            classify_verdict(
                "map_decl",
                "other",
                "subset_julia_vm/src/aot/ir/aot_types.rs",
                "by_name",
            ),
            IDENTITY_BEARING,
        )

    def test_phase_2a_qualified_tables_are_lexical_boundaries(self) -> None:
        for symbol in (
            "module_functions",
            "module_exports",
            "module_constants",
            "module_struct_names",
            "module_usings",
            "module_abstract_names",
            "module_imported_bindings",
            "module_aliases",
        ):
            with self.subTest(symbol=symbol):
                self.assertEqual(
                    classify_verdict(
                        "map_decl",
                        "module",
                        "subset_julia_vm/src/compile/context.rs",
                        symbol,
                    ),
                    LEXICAL_BOUNDARY,
                )

    def test_phase_2a_verified_inert_tables_are_inert(self) -> None:
        for symbol in (
            "global_types",
            "inference_global_types",
            "global_const_structs",
            "global_struct_names",
        ):
            with self.subTest(symbol=symbol):
                self.assertEqual(
                    classify_verdict(
                        "map_decl",
                        "global",
                        "subset_julia_vm/src/compile/context.rs",
                        symbol,
                    ),
                    INERT,
                )

    def test_name_to_id_indexes_are_exact_path_lexical_boundaries(self) -> None:
        boundaries = (
            ("subset_julia_vm_bytecode/src/module_intern.rs", "index", "module"),
            ("subset_julia_vm_bytecode/src/struct_registry.rs", "by_name", "struct"),
            (
                "subset_julia_vm_types/src/inference_core/type_core/match.rs",
                "by_name",
                "typevar",
            ),
        )
        for path, symbol, domain in boundaries:
            with self.subTest(path=path, symbol=symbol):
                self.assertEqual(
                    classify_verdict("map_decl", domain, path, symbol),
                    LEXICAL_BOUNDARY,
                )

        self.assertEqual(
            classify_verdict(
                "map_decl",
                "typevar",
                "subset_julia_vm_types/src/inference_core/type_core/substitute.rs",
                "by_name",
            ),
            IDENTITY_BEARING,
        )

    def test_tsv_aggregation_keeps_verdicts_separate(self) -> None:
        rows = [
            (
                "map_decl",
                "module",
                "compile",
                "requires-owner-context-plumbing",
                IDENTITY_BEARING,
                "subset_julia_vm/src/compile/context.rs",
                10,
                "method_tables: MethodTable",
            ),
            (
                "map_decl",
                "module",
                "compile",
                "requires-owner-context-plumbing",
                LEXICAL_BOUNDARY,
                "subset_julia_vm/src/compile/context.rs",
                20,
                "module_functions: HashSet<String>",
            ),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "inventory.tsv"
            write_tsv(output, rows)
            lines = output.read_text(encoding="utf-8").splitlines()

        self.assertEqual(
            lines[0],
            "kind\tdomain\tlayer\tdifficulty\tverdict\tmodule\tcount",
        )
        self.assertTrue(any("\tidentity-bearing\t" in line for line in lines[1:]))
        self.assertTrue(any("\tlexical-boundary\t" in line for line in lines[1:]))
        self.assertEqual(len(lines), 3)


if __name__ == "__main__":
    unittest.main()
