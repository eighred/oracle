"""Cross-check the Rust CLI with direct full repricing and equal-probability tickets."""
from fractions import Fraction
import json
from pathlib import Path
import subprocess
import unittest


def report(known, mode="--risk-bps", valid=1767225660000):
    run = subprocess.run(
        ["cargo", "run", "--locked", "--quiet", "-p", "oracle-portfolio-replay",
         "--", mode, "6250", str(valid), str(known)],
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

    def test_frozen_daily_forecast_against_separate_outcome_revisions(self):
        start = 1767225660000
        end = start + 24 * 60 * 60 * 1000
        frozen = report(start + 1_000)
        self.assertEqual(frozen["snapshot"]["observation_id"], "bar-a")
        self.assertEqual((frozen["value_at_risk"], frozen["expected_shortfall"]), ("2", "20/3"))
        for known, mark, observation in [
            (end + 300, Fraction("23.50"), "next-day-close"),
            (end + 120_000, Fraction("24"), "next-day-correction"),
        ]:
            with self.subTest(known=known):
                actual = report(known, "--score-daily-risk-bps")
                self.assertEqual(actual["forecast"], frozen)
                self.assertEqual(actual["score_known_at_ms"], known)
                self.assertEqual(actual["outcome_valid_at_ms"], end)
                self.assertEqual((actual["scheduled_intervals"], actual["scored_intervals"]), (1, 1))
                self.assertIn("insufficient", actual["uncertainty"])
                result = actual["score"]
                self.assertEqual(result["observation_id"], observation)
                self.assertEqual(result["received_ms"], known)
                self.assertEqual(Fraction(result["closing_mark"]), mark)
                closing_nav = 50 + 2 * mark
                loss = 100 - closing_nav
                self.assertEqual(Fraction(result["closing_nav"]), closing_nav)
                self.assertEqual(Fraction(result["portfolio_pnl"]), -loss)
                self.assertEqual(Fraction(result["loss"]), loss)
                self.assertEqual(result["var_exceeded"], loss > 2)

        for valid, known, reason in [
            (start, end - 1, "horizon_not_elapsed"),
            (start, end + 299, "outcome_unavailable_at_cutoff"),
            (start + 60_000, end + 120_000, "outcome_unavailable_at_cutoff"),
            (start + 120_000, end + 180_000, "incomplete_portfolio_snapshot"),
        ]:
            with self.subTest(valid=valid, known=known):
                actual = report(known, "--score-daily-risk-bps", valid)
                self.assertEqual((actual["scheduled_intervals"], actual["scored_intervals"]), (1, 0))
                self.assertEqual(actual["score"], {"state": "unavailable", "reason": reason})


if __name__ == "__main__":
    unittest.main()
