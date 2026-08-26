## Why

Now that `BillingStatement__c` is wired to the real mirror schema, two fidelity gaps in the renewal (dues) Relationship Anchor are visible. Billing statements only exist from FY2023 onward, yet the churn model treats the renewal feature family as "covered" in every training year because coverage is measured on whether the source is *available*, not on whether that fiscal year actually has dues data. So the majority of training years feed the model a constant, information-free dues feature that the coverage gate cannot see. Separately, dues settlement is labeled from whole-statement totals that also include security fees, tuition, and gifts, overstating what the label claims to describe.

## What Changes

- Make optional-family coverage **data-aware**: a feature family counts as observed for a household-year only when that fiscal year actually carries source data for the family, so the existing 70% coverage / 15-point drift gate can restrict or drop a family for the years that lack data — using the current thresholds unchanged.
- Derive dues **settlement** from the qualifying membership-dues lines (per-line balance/received/status now present on `BillingStatementLine__c`) rather than from statement-level totals that mix in non-dues products.
- No change to the prediction target, cutoff-safety rules, quality gates, gate thresholds, privacy/audit boundaries, or the other feature families. Settlement remains an eventual-state display label, never a model feature.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `validated-membership-risk`: the optional-source coverage requirement changes so coverage is measured on per-fiscal-year source-data presence rather than whole-source availability, letting the unchanged coverage/drift gate exclude a family for years without data.
- `membership-lifecycle-insights`: the membership-dues evidence requirement changes so eventual settlement is computed from qualifying dues lines, not from whole-statement totals spanning non-dues products.

## Impact

- **Code (Rust)**: `src-tauri/src/risk.rs` (`family_observed`/`coverage` and the `evaluate` split so coverage reflects per-year data presence); `src-tauri/src/insights.rs` (`apply_dues`/`dues_evidence`/`statement_settlement` to base settlement on dues lines, using `Billing_Balance__c` / `Billing_ReceivedAmount__c` / `Billing_Status__c`).
- **Privacy/Audit**: none — no new data surfaced, no change to named-list access or audit.
- **Source data**: none — reads the same already-synced mirror objects; no schema or sync change.
- **Dependencies**: none added.
- **Results**: churn-model feature set may drop or narrow the renewal family for pre-FY2023 years; aggregate dues counts and named rankings for years with data are unchanged. This is separate from `optimize-insights-load`, which covers the per-household scan performance issue.
