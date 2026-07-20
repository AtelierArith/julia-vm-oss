from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]


def load_report_module():
    path = REPO_ROOT / "scripts" / "ios_e2e_report.py"
    spec = importlib.util.spec_from_file_location("ios_e2e_report", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class IOSReportingTests(unittest.TestCase):
    def test_ios_e2e_report_normalizes_legacy_statuses(self):
        report = load_report_module()

        self.assertEqual(report.normalize_status("PASS"), "sample_pass")
        self.assertEqual(report.normalize_status("DONE"), "sample_pass")
        self.assertEqual(report.normalize_status("FAIL"), "sample_fail")
        self.assertEqual(report.normalize_status("UNKNOWN"), "infra_failure")
        self.assertEqual(report.normalize_status("ERROR"), "infra_failure")

    def test_ios_e2e_report_retries_only_infra_failures(self):
        report = load_report_module()

        self.assertTrue(report.should_retry("infra_failure", attempt=1, max_attempts=2))
        self.assertFalse(report.should_retry("infra_failure", attempt=2, max_attempts=2))
        self.assertFalse(report.should_retry("sample_fail", attempt=1, max_attempts=2))
        self.assertFalse(report.should_retry("sample_pass", attempt=1, max_attempts=2))

    def test_ios_e2e_report_parses_counts_without_mixing_infra(self):
        report = load_report_module()
        with tempfile.TemporaryDirectory() as tmpdir:
            report_file = Path(tmpdir) / "report.txt"
            report_file.write_text(
                "\n".join(
                    [
                        "sample_pass\tplotting_2d\t/tmp/plot.png",
                        "sample_fail\tode\tMethodError (/tmp/ode.png)",
                        "infra_failure\trepl\tAX wedge after Simulator restart",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            summary = report.parse_report(report_file)

        self.assertEqual(summary.sample_pass, 1)
        self.assertEqual(summary.sample_fail, 1)
        self.assertEqual(summary.infra_failure, 1)
        self.assertEqual(summary.sample_total, 2)
        self.assertEqual(summary.total, 3)
        self.assertEqual(summary.sample_rate, 50.0)
        self.assertEqual(summary.infra_rate, 33.33)

    def test_ios_e2e_report_summary_cli_is_shell_parseable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            report_file = Path(tmpdir) / "report.txt"
            report_file.write_text(
                "\n".join(
                    [
                        "sample_pass\tplotting_2d\t/tmp/plot.png",
                        "sample_fail\tode\tMethodError (/tmp/ode.png)",
                        "infra_failure\trepl\tAX wedge after Simulator restart",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            cp = subprocess.run(
                [
                    sys.executable,
                    str(REPO_ROOT / "scripts" / "ios_e2e_report.py"),
                    "--summary",
                    str(report_file),
                ],
                check=True,
                text=True,
                capture_output=True,
            )

        self.assertIn("sample_pass=1", cp.stdout)
        self.assertIn("sample_fail=1", cp.stdout)
        self.assertIn("infra_failure=1", cp.stdout)
        self.assertIn("sample_rate=50.00", cp.stdout)
        self.assertIn("infra_rate=33.33", cp.stdout)


if __name__ == "__main__":
    unittest.main()
