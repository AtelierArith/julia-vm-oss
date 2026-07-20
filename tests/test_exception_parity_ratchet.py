import importlib.util
import sys
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "exception_parity_ratchet.py"
SPEC = importlib.util.spec_from_file_location("exception_parity_ratchet", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


HEADER = (
    "id\tcategory\trelated_issue\tjulia_exit\tjulia_catchable\t"
    "julia_exc_type\tsjulia_exit\tsjulia_catchable\tsjulia_exc_type\t"
    "type_match\tcatchable_match\tnote\tjulia_health\tsjulia_health\n"
)


class ExceptionParityRatchetTests(unittest.TestCase):
    def check(self, report_rows: str, allowlist_rows: str):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            report = root / "report.tsv"
            allowlist = root / "allowlist.tsv"
            enriched_rows = "".join(
                line + "\tok\tok\n"
                for line in report_rows.rstrip("\n").splitlines()
                if line
            )
            report.write_text(HEADER + enriched_rows, encoding="utf-8")
            allowlist.write_text(
                "id\tissue\tclass\treason\n" + allowlist_rows,
                encoding="utf-8",
            )
            return MODULE.check_ratchet(report, allowlist)

    def test_matching_issue_linked_divergence_is_green(self):
        result = self.check(
            "known\tnumeric\t#123\t1\tyes\tDomainError\t0\t"
            "no-exception-raised\t\tno\tno\tknown gap\n",
            "known\t#123\tsilent-error\ttracked gap\n",
        )
        self.assertEqual([], result.errors)

    def test_new_divergence_is_rejected(self):
        result = self.check(
            "new_gap\tnumeric\t\t1\tyes\tDomainError\t0\t"
            "no-exception-raised\t\tno\tno\tregression\n",
            "",
        )
        self.assertTrue(any("NEW unallowlisted" in error for error in result.errors))

    def test_stale_allowlist_is_rejected(self):
        result = self.check(
            "fixed\tnumeric\t#123\t1\tyes\tDomainError\t1\tyes\t"
            "DomainError\tyes\tyes\tfixed\n",
            "fixed\t#123\ttype\told gap\n",
        )
        self.assertTrue(any("STALE allowlist" in error for error in result.errors))

    def test_allowlist_requires_issue_number(self):
        result = self.check(
            "known\tnumeric\t\t1\tyes\tDomainError\t0\t"
            "no-exception-raised\t\tno\tno\tknown gap\n",
            "known\tTODO\tsilent-error\tmissing owner\n",
        )
        self.assertTrue(any("issue must be #<number>" in error for error in result.errors))

    def test_corpus_shrink_below_floor_is_rejected(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            report = root / "report.tsv"
            allowlist = root / "allowlist.tsv"
            report.write_text(
                HEADER
                + "only\tnumeric\t\t1\tyes\tMethodError\t1\tyes\t"
                "MethodError\tyes\tyes\t-\tok\tok\n",
                encoding="utf-8",
            )
            allowlist.write_text(
                "id\tissue\tclass\treason\n", encoding="utf-8"
            )
            result = MODULE.check_ratchet(report, allowlist, minimum_cases=2)
        self.assertTrue(any("corpus shrank" in error for error in result.errors))

    def test_unknown_match_token_is_rejected(self):
        result = self.check(
            "broken\tnumeric\t\t1\tyes\tMethodError\t1\tyes\t"
            "MethodError\tERROR\tERROR\t-\n",
            "",
        )
        self.assertTrue(any("invalid type_match" in error for error in result.errors))

    def test_unknown_catchability_token_is_rejected(self):
        result = self.check(
            "broken\tnumeric\t\t1\tERROR\tMethodError\t1\tERROR\t"
            "MethodError\tyes\tyes\t-\n",
            "",
        )
        self.assertTrue(any("invalid julia_catchable" in error for error in result.errors))

    def test_allowlisted_divergence_cannot_worsen_class(self):
        result = self.check(
            "known\tnumeric\t#123\t1\tyes\tDomainError\t1\t"
            "no-uncatchable\t\tno\tno\tworsened\n",
            "known\t#123\tsilent-error\ttracked gap\n",
        )
        self.assertTrue(any("divergence class changed" in error for error in result.errors))

    def test_case_identity_baseline_rejects_same_count_substitution(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            report = root / "report.tsv"
            baseline = root / "baseline.tsv"
            allowlist = root / "allowlist.tsv"
            row = "\tnumeric\t\t1\tyes\tMethodError\t1\tyes\tMethodError\tyes\tyes\t-\tok\tok\n"
            report.write_text(HEADER + "replacement" + row, encoding="utf-8")
            baseline.write_text(HEADER + "sentinel" + row, encoding="utf-8")
            allowlist.write_text("id\tissue\tclass\treason\n", encoding="utf-8")
            result = MODULE.check_ratchet(
                report, allowlist, case_baseline_path=baseline
            )
        self.assertTrue(any("corpus case identity drift" in error for error in result.errors))

    def test_infrastructure_failure_never_counts_as_parity(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            report = root / "report.tsv"
            allowlist = root / "allowlist.tsv"
            report.write_text(
                HEADER
                + "timed_out\tcontrol_flow\t\t-9\tno-uncatchable\t\t-9\t"
                "no-uncatchable\t\tyes\tyes\tprobe timed out\ttimeout\ttimeout\n",
                encoding="utf-8",
            )
            allowlist.write_text("id\tissue\tclass\treason\n", encoding="utf-8")
            result = MODULE.check_ratchet(report, allowlist)
        self.assertTrue(any("infrastructure failure" in error for error in result.errors))

    def test_health_columns_are_required(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            report = root / "report.tsv"
            allowlist = root / "allowlist.tsv"
            report.write_text(
                HEADER.replace("\tjulia_health\tsjulia_health", ""),
                encoding="utf-8",
            )
            allowlist.write_text("id\tissue\tclass\treason\n", encoding="utf-8")
            result = MODULE.check_ratchet(report, allowlist)
        self.assertTrue(any("missing required column" in error for error in result.errors))

    def test_full_suite_premerge_wires_runtime_ratchet(self):
        proc = subprocess.run(
            ["bash", "scripts/premerge_gate.sh", "--list-gates", "--full-suite"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertIn("bash scripts/exception_parity_ratchet.sh", proc.stdout)


if __name__ == "__main__":
    unittest.main()
