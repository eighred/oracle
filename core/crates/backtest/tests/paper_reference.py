"""Reconcile an exported complete forward paper experiment using exact fractions."""
import hashlib
import json
from fractions import Fraction as F
from pathlib import Path
import sys


def check(report):
    plan = report["manifest"]
    first = plan["first_ms"]
    source = plan["source_kind"]
    assert source in {"prospective_synthetic", "ccxt_loopback_fixture", "ccxt_binance_spot"}
    assert report["state"] == "complete"
    canonical = json.dumps(plan, separators=(",", ":"), sort_keys=True).encode()
    assert hashlib.sha256(canonical).hexdigest() == report["manifest_sha256"]
    assert report["frozen_ms"] < first and plan["benchmark"] == "hold"
    samples = report["samples"]
    marks = [F(s["mark"]) for s in samples]
    times = report["recorded_ms"]
    intents = report["committed_instructions"]
    assert len(samples) == len(times) == len(intents) == 9 and not report["unobserved_slots"]
    for i, (s, mark, recorded) in enumerate(zip(samples, marks, times, strict=True)):
        scheduled = first + 60_000*i
        assert s["close_ms"] == scheduled <= s["known_ms"] <= recorded < scheduled+15_000
        assert mark > 0 and (mark*100).denominator == 1
        if source == "prospective_synthetic":
            assert 23 <= mark <= 27
        else:
            ref = report["source_receipts"][i]
            assert ref["open_ms"] + 60_000 == ref["close_ms_inclusive"] + 1 == scheduled
            assert ref["received_ms"] == s["known_ms"] and len(bytes.fromhex(ref["response_json_sha256"])) == 32
    assert plan["last_ms"] == first+8*60_000 and plan["funding"] == plan["external_flows"] == "0"
    assert intents[:2] == [{}, {}] and intents[-1] == {}
    assert set(run["rule"] for run in report["projection"]["runs"]) == {"hold", "dca", "momentum"}
    for run in report["projection"]["runs"]:
        rule = run["rule"]
        cash, qty, basis, realized = F(50), F(2), 2*marks[2], F(0)
        opening = cash+basis
        peak, worst, fees, traded = opening, F(0), F(0), F(0)
        assert F(run["opening_nav"]) == opening and len(run["ledger"]) == 6
        for i, row in enumerate(run["ledger"], 3):
            mark = marks[i]
            direction = (marks[i-1] > marks[i-2]) - (marks[i-1] < marks[i-2])
            side = {"momentum": direction, "dca": int((i-3)%2 == 0), "hold": 0}[rule]
            assert intents[i-1][rule] == {-1:"sell", 0:None, 1:"buy"}[side]
            assert row["decision_ms"] == samples[i-1]["known_ms"] <= times[i-1] < row["valuation_close_ms"]
            assert row["valuation_known_ms"] == samples[i]["known_ms"]
            assert F(row["signal_mark"]) == marks[i-1] and F(row["valuation_mark"]) == mark
            assert row["valuation_close_ms"] == samples[i]["close_ms"]
            if side:
                signed_qty = F(side,4)
                ticks = mark*(1+side*F(5,10000))*100
                rounded = -(-ticks//1) if side > 0 else ticks//1
                price = F(rounded,100)
                notional = price/4
                fee = F(-(-(notional*100_000)//1),100_000_000)
                reason = ("insufficient_cash" if side > 0 and cash < notional+fee else
                          "insufficient_quantity" if side < 0 and qty < -signed_qty else None)
                if reason:
                    assert row["posting"] == {"state":"rejected","side":intents[i-1][rule],"reason":reason}
                else:
                    fill = row["posting"]["fill"]
                    assert fill["side"] == intents[i-1][rule] and F(fill["price"]) == price
                    assert F(fill["fee"]) == fee and F(fill["quantity_delta"]) == signed_qty
                    assert F(fill["cash_delta"]) == -signed_qty*price-fee
                    assert F(fill["half_spread_cost"]) == mark*F(2,40000)
                    assert F(fill["slippage_cost"]) == mark*F(3,40000)
                    assert F(fill["tick_rounding_cost"]) == abs(price-ticks/100)/4
                    if side > 0:
                        basis += notional + fee
                    else:
                        removed = basis * (-signed_qty) / qty
                        realized += notional - fee - removed
                        basis -= removed
                    cash -= signed_qty*price + fee
                    qty += signed_qty
                    fees += fee
                    traded += notional
            else:
                assert row["posting"]["state"] == "hold"
            nav = cash+qty*mark
            peak = max(nav,peak)
            dd = (peak-nav)/peak
            worst = max(worst,dd)
            assert [F(row[k]) for k in ("cash","quantity","nav","realized_pnl","unrealized_pnl","drawdown")] == [
                cash, qty, nav, realized, qty*mark-basis, dd]
        assert [F(run[k]) for k in ("closing_nav","net_pnl","total_fees","max_drawdown","turnover_over_opening_nav")] == [
            nav,nav-opening,fees,worst,traded/opening]
        if source != "prospective_synthetic":
            ref = report["opening_valuation"]
            assert F(ref["nav"]) == opening and F(ref["inventory_value"]) == 2*marks[2]
            assert ref["observation_sha256"] == report["source_receipts"][2]["response_json_sha256"]
            assert ref["valid_ms"] == samples[2]["close_ms"] and ref["known_ms"] == samples[2]["known_ms"]
            losses = sorted(qty*marks[-1]*(1-after/before) for before, after in zip(marks, marks[1:]))
            risk = report["minute_risk"]
            assert risk["horizon_ms"] == 60_000 and risk["sample_size"] == 8 and risk["confidence_bps"] == 6250
            tail = risk["by_rule"][rule]
            assert F(tail["quantity"]) == qty
            assert F(tail["value_at_risk"]) == losses[4]
            assert F(tail["expected_shortfall"]) == sum(losses[5:])/3



if __name__ == "__main__":
    check(json.loads(Path(sys.argv[1]).read_bytes()))
    print("Verified all nine receipts, committed instructions, eighteen settlements and three net returns.")
