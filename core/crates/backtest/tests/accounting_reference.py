"""Reprice the held-out paths independently with Decimal and compare the operator report."""
from concurrent.futures import ThreadPoolExecutor
from decimal import Decimal as D, ROUND_CEILING, ROUND_FLOOR
from fractions import Fraction
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


CORE = Path(__file__).resolve().parents[3]
MARKS = list(map(D, ["25", "26", "25", "27", "26", "26", "24", "25", "23"]))
# Frozen before the Rust implementation, in holdout settlement order.
ORDERS = {"hold": [0, 0, 0, 0, 0, 0], "dca": [1, 0, 1, 0, 1, 0],
          "momentum": [-1, 1, -1, 0, -1, 1]}
NET = {"hold": "-4", "dca": "-6.284515", "momentum": "-2.556745"}


def command(*args):
    return subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "oracle-backtest", "--", *args],
                          cwd=CORE, capture_output=True)


class BacktestReference(unittest.TestCase):
    def test_every_cash_and_inventory_posting_and_summary(self):
        result = command()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["opening_snapshot"]["nav"], "100")
        self.assertEqual({run["rule"] for run in report["runs"]}, ORDERS.keys())
        for run in report["runs"]:
            with self.subTest(rule=run["rule"]):
                cash, btc, peak = D(50), D(2), D(100)
                fees = traded = D(0)
                worst = Fraction(0)
                for side, mark, row in zip(ORDERS[run["rule"]], MARKS[3:], run["ledger"], strict=True):
                    self.assertLess(row["decision_ms"], row["valuation_close_ms"])
                    self.assertLessEqual(row["valuation_close_ms"], row["valuation_known_ms"])
                    self.assertEqual(D(row["valuation_mark"]), mark)
                    if side:
                        quantity = D(side) / 4
                        px = (mark * (1 + side * D("0.0005"))).quantize(
                            D("0.01"), rounding=ROUND_CEILING if side > 0 else ROUND_FLOOR)
                        notional = abs(quantity * px)
                        fee = (notional / 1000).quantize(D("0.00000001"), rounding=ROUND_CEILING)
                        fill = row["posting"]["fill"]
                        self.assertEqual(fill["side"], "buy" if side > 0 else "sell")
                        self.assertEqual(D(fill["price"]), px)
                        self.assertEqual(D(fill["fee"]), fee)
                        adverse = sum(D(fill[k]) for k in ("half_spread_cost", "slippage_cost", "tick_rounding_cost"))
                        self.assertEqual(adverse, quantity * (px - mark))
                        self.assertEqual(D(fill["cash_delta"]), -quantity * px - fee)
                        self.assertEqual(D(fill["quantity_delta"]), quantity)
                        cash -= quantity * px + fee
                        btc += quantity
                        fees += fee
                        traded += notional
                    else:
                        self.assertEqual(row["posting"]["state"], "hold")
                    nav = cash + btc * mark
                    peak = max(peak, nav)
                    dd = Fraction(peak - nav) / Fraction(peak)
                    worst = max(worst, dd)
                    self.assertEqual(Fraction(row["drawdown"]), dd)
                    self.assertEqual([D(row[k]) for k in ("cash", "quantity", "nav")], [cash, btc, nav])
                    self.assertEqual(Fraction(row["realized_pnl"]) + Fraction(row["unrealized_pnl"]),
                                     Fraction(nav - 100))
                self.assertEqual(D(run["net_pnl"]), D(NET[run["rule"]]))
                self.assertEqual(D(run["closing_nav"]), nav)
                self.assertEqual(D(run["total_fees"]), fees)
                self.assertEqual(Fraction(run["turnover_over_opening_nav"]), Fraction(traded) / 100)
                self.assertEqual(Fraction(run["max_drawdown"]), worst)

    def test_replay_and_concurrent_report_publication(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder, "report.json")
            with ThreadPoolExecutor(max_workers=2) as pool:
                writes = list(pool.map(lambda _: command("--output", path), range(2)))
            for write in writes:
                self.assertEqual(write.returncode, 0, write.stderr)
            result = command()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(path.read_bytes(), result.stdout)
            path.write_bytes(b"previous report")
            self.assertNotEqual(command("--output", path).returncode, 0)
            self.assertEqual(path.read_bytes(), b"previous report")


if __name__ == "__main__":
    unittest.main()
