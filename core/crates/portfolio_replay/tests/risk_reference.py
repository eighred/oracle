"""Cross-check the Rust CLI with direct full repricing and equal-probability tickets."""
from fractions import Fraction
import json
from pathlib import Path
import subprocess
import unittest


def report(known):
    run = subprocess.run(
        ["cargo", "run", "--locked", "--quiet", "-p", "oracle-portfolio-replay",
         "--", "--risk-bps", "6250", "1767225660000", str(known)],
        cwd=Path(__file__).resolve().parents[3], capture_output=True, check=True, text=True,
    )
    return json.loads(run.stdout)


class HistoricalRiskReference(unittest.TestCase):
    def test_original_and_restatement_against_eight_tickets(self):
        levels = list(map(Fraction, ["100", "100", "98", "94.08", "77.1456"]))
        for known, mark in [(1767225660100, Fraction(25)), (1767225780000, Fraction("25.50"))]:
            with self.subTest(known=known):
                actual = report(known)
                nav = 50 + 2 * mark
                losses = [nav - (50 + 2 * mark * b / a) for a, b in zip(levels, levels[1:])]
                tickets = sorted(losses * 2)
                self.assertEqual(actual["risk_coverage"], "complete")
                self.assertEqual(actual["sample_size"], 4)
                self.assertEqual(actual["snapshot"]["quote"], "USDT")
                self.assertEqual(Fraction(actual["snapshot"]["nav"]), nav)
                self.assertEqual([Fraction(s["loss"]) for s in actual["scenarios"]], losses)
                self.assertEqual(Fraction(actual["value_at_risk"]), tickets[4])
                self.assertEqual(Fraction(actual["expected_shortfall"]), sum(tickets[5:]) / 3)
        early = report(1767225660100)
        self.assertEqual((early["value_at_risk"], early["expected_shortfall"]), ("2", "20/3"))
        self.assertEqual(early, report(1767225660100))

    def test_missing_mark_withholds_tail_estimates(self):
        actual = report(1767225660099)
        self.assertEqual(actual["risk_coverage"], "incomplete")
        self.assertEqual(actual["reason"], "incomplete_portfolio_snapshot")
        self.assertNotIn("expected_shortfall", actual)


if __name__ == "__main__":
    unittest.main()
