# Retention Dashboard — Analyst Notes (Q3)

Author: Dana K., Growth Analytics
Date: 2026-04-08
Source: internal BI dashboard, `retention.loom-metrics/channels`, snapshot 2026-04-05

## Q3 30-day retention by acquisition channel

| Channel   | Q1 2026 | Q2 2026 | Q3 2026 |
|-----------|---------|---------|---------|
| Referral  | 38%     | 40%     | 42%     |
| Organic   | 33%     | 34%     | 35%     |
| Paid      | 29%     | 28%     | 27%     |

Referral is the strongest channel and has been trending up every quarter. Paid is
the only channel trending down, which matches what support has been flagging about
paid-acquired users churning faster (see ticket summary).

Note the dashboard export used here is the **finance-reconciled** cut — it excludes
accounts that never completed onboarding, since finance doesn't count them as
"acquired" for retention purposes. Growth's raw funnel numbers (unreconciled) may
read a few points lower per channel. I haven't cross-checked this export against the
raw funnel pull this quarter — flagging in case someone else already did and got a
different number for referral.

## Open question

Should we weight the Q4 renewal push toward referral (highest retention) or organic
(highest volume)? Need a decision before the Q4 planning doc is due.
