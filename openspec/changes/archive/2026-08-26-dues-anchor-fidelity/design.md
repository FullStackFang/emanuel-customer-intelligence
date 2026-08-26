## Context

The Relationship Anchor adapter in `insights.rs` now reads the real optional-source schema. Two properties of the churn pipeline in `risk.rs` were designed when those sources were unsynced and no longer hold:

- `family_observed(row, family)` returns the family's source-availability flag (`has_renewal` / `has_school` / `has_committee`), which `build_feature_row` sets from `cap_available(caps, key)` — a single value for the whole run. `coverage()` therefore reports 1.0 for an available source in every fiscal year, even years with no rows in that source. Billing statements begin at FY2023 (`Date__c`), so ~9 of the ~12 modeled cutoff years carry a constant renewal feature that the coverage/drift gate cannot detect.
- Dues settlement is computed in `statement_settlement` from `BillingStatement__c.TotalBalance__c` / `TotalCredits__c`, which sum every product on the statement (security fees, tuition, gifts). The label claims to describe dues settlement but measures the whole statement.

Constraints: analytics read only the encrypted local mirror; no source or mart schema change; keep the change Rust-only and surgical; do not touch the prediction target, cutoff-safety rules, gate thresholds, or privacy/audit paths.

## Goals / Non-Goals

**Goals:**
- Coverage reflects real per-fiscal-year data presence for each optional family, so the existing 70% / 15-point gate can drop or restrict a family for the years that lack data.
- Dues settlement is derived from the qualifying dues lines only.
- No change to model output for years and families that already have complete data.

**Non-Goals:**
- Changing gate thresholds (`MIN_COVERAGE`, `MAX_DRIFT`), the target, cutoff rules, or the school/committee anchor logic.
- Backfilling dues history before FY2023, or any new sync.
- The O(n×m) per-household scan performance issue (owned by `optimize-insights-load`).
- Any frontend change beyond the settlement label already rendered.

## Decisions

**1. Measure coverage on per-year data presence, not source availability.**
Record, per household-year, whether that family's source contributed any row for that fiscal year, and make `family_observed` consult that instead of the run-wide capability flag. The simplest carrier is a per-row observed flag set during anchor application: when `apply_dues` runs, mark each touched household-year as renewal-observed for its fiscal year; a year the adapter never touches (no statements exist) stays unobserved. `coverage()` then naturally falls below 70% across a training window dominated by pre-FY2023 years, and the existing removal loop in `evaluate` drops the family and revalidates — no gate-threshold change.
- *Alternative considered:* derive per-year presence from a separate "which fiscal years have source rows" query. Rejected — the adapter already visits exactly the rows that carry data, so a per-row observed flag is cheaper and cannot drift from what actually populated the anchors.
- *Distinction to preserve:* "observed but no dues line" (active household, source present that year → `dues_coverage_missing`) must stay separate from "family unobserved that year" (no source data at all). The former remains a real feature signal; only the latter lowers coverage.

**2. Base settlement on dues lines.**
Compute settlement from the qualifying membership-dues lines using the per-line `Billing_Balance__c` / `Billing_ReceivedAmount__c` (and `Billing_Status__c` where clearer) already on `BillingStatementLine__c`, replacing the statement-total inputs in `statement_settlement`. Settlement stays an eventual-state display label, never a model feature, so this changes only the label's fidelity.
- *Alternative considered:* keep statement totals but subtract non-dues lines. Rejected — the per-line fields give the dues figures directly and avoid re-deriving them by subtraction.

## Risks / Trade-offs

- **Renewal family gets dropped for the whole model, losing a signal in FY2023+ where it is real.** → Coverage is computed over the training window; document that dues becomes useful only once several post-FY2023 years exist. The gate behavior is the intended, conservative outcome until then; no threshold change hides it.
- **Confusing "unobserved year" with "coverage missing" would corrupt the `dues_coverage_missing` feature.** → Keep the two concepts as distinct fields/flags with a dedicated test asserting an active FY2024 household with no dues line is `coverage_missing` yet renewal-observed, while an FY2018 household is renewal-unobserved.
- **Per-line settlement fields may be null on some lines.** → Fall back to the existing Unknown state when the dues lines lack balance/received values, exactly as statement-level Unknown behaves today.
- **Regression risk to existing risk tests.** → Add a before/after equivalence check on a synthetic dataset where every year has data (coverage stays 1.0), proving identical model output.
