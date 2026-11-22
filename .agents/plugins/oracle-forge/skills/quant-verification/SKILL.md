---
name: quant-verification
description: Independently verify Oracle valuation, portfolio risk, strategy backtest and paper-ledger claims.
---
# Verify a financial result

Record dataset version, as-of and known-at cutoffs, base currency, precision, rounding, model version and cost assumptions. Choose an independent hand calculation or separately derived reference and a tolerance tied to the quantity before running the candidate. Reconcile cash, position and realized versus unrealized changes separately.

For time-series validation freeze the evaluation interval and benchmark, fit only on earlier data and account for fees, funding and slippage. Change a future observation and confirm earlier decisions stay fixed. Compare a replay to its original run, including drawdown and net-of-cost results. Report simulated outcomes as simulated even when every check passes. Separate technical evidence from permission to deploy capital.
