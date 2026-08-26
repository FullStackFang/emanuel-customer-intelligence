## 1. Data-aware coverage: failing tests first

- [x] 1.1 Add a `risk.rs` test proving the regression: a synthetic dataset where an optional family has data only in the latest fiscal years (below 70% per-year coverage across the training window) must remove that family in `evaluate`; today it is kept because the source is available.
- [x] 1.2 Add a `risk.rs` test that pins the distinction: an active household-year with the source present but no dues line stays `dues_coverage_missing` (a real feature) AND renewal-observed, while a year with no statement data at all is renewal-unobserved.
- [x] 1.3 Add an equivalence test: a synthetic dataset where every year has data for every family keeps coverage at 1.0 and produces byte-identical model columns/output vs. the pre-change behavior.

## 2. Data-aware coverage: implementation

- [x] 2.1 Carry per-fiscal-year family observation into the model: add per-row observed flags (e.g. `renewal_observed`/`school_observed`/`committee_observed` on `HhFy`, or an equivalent lookup) set true only for the household-years the anchor adapters actually touch in `insights.rs` (`apply_dues`/`apply_school`/`apply_committee`).
- [x] 2.2 Thread the per-year observation into `FeatureRow` and change `family_observed` (`risk.rs:428`) to consult it instead of the run-wide `has_renewal`/`has_school`/`has_committee` capability flags; keep `has_*` for reporting only.
- [x] 2.3 Confirm `coverage`, `evaluate`'s removal loop, and `MIN_COVERAGE`/`MAX_DRIFT` are untouched — only the observation input changes — so no gate threshold moves.
- [x] 2.4 Keep `dues_coverage_missing` derivation in `apply_dues` exactly as is (source-present year, no dues line); ensure it is independent of the new unobserved-year flag.
- [x] 2.5 Run the tests from section 1 to green.

## 3. Dues-line settlement: failing tests first

- [x] 3.1 Add an `insights.rs` test: a statement carrying both a dues line and a non-dues charge (e.g. security fee) yields settlement computed from the dues line's balance/received only, not the statement total.
- [x] 3.2 Add a test that a dues line with null per-line balance/received falls back to the existing `SettlementState::Unknown`.

## 4. Dues-line settlement: implementation

- [x] 4.1 Extend `BillingStatementLine` and `LINE_FIELDS` (`insights.rs`) to read `Billing_Balance__c` / `Billing_ReceivedAmount__c` (and `Billing_Status__c` if clearer), pinned to the confirmed schema.
- [x] 4.2 Change `dues_evidence`/`statement_settlement` (`insights.rs:263,294`) to derive settlement from the qualifying dues lines rather than `TotalBalance__c`/`TotalCredits__c`; preserve the `settled`/`partially settled`/`unsettled`/`unknown` mapping and the eventual-state label wording.
- [x] 4.3 Update the existing `dues_evidence_labels_final_mirror_values_as_eventual_settlement` test and the `anchors_populate_from_optional_mirror_sources` fixture for the line-level inputs.
- [x] 4.4 Run the tests from section 3 to green.

## 5. Verification

- [x] 5.1 `cargo fmt`, `cargo clippy`, and `cargo test --lib` all green.
- [x] 5.2 Rebuild the mart against the real local mirror (release) and confirm: renewal family behavior matches the new coverage rule for pre-FY2023 years, dues counts for FY2023+ are unchanged, and settlement labels shift only where non-dues charges shared a statement.
- [x] 5.3 Record the rolling backtest coverage report before/after to confirm the renewal family is handled by the gate as designed and no other family or gate result changed.
- [x] 5.4 Confirm the Risk and Renewal & Engagement views render unchanged except the settlement label fidelity; no privacy/audit path touched.

## Verification notes (2026-08-26, real local mirror, release rebuild on a scratch copy)

- 13,030 households / 160,954 household-years; anchors, `coverage_missing`, and activity are identical per household-year before and after. FY2023+ billed counts unchanged (294 / 1780 / 1842 / 1868 / 1921).
- `renewal_observed` is 0 for FY≤2022 and 100% for FY2023+ (statements exist for calendar 2023–2026 only). `school_observed` begins FY2019; committee covers every year.
- Coverage gate after: **renewal removed as designed, and school also removed** (enrolment data starts FY2019 → ~46% coverage across the FY2012–2024 training window). The proposal anticipated only renewal moving; school is the same rule applied to real data. Committee stays at 1.0. Model metrics AUC 0.669→0.696, lift 2.41→2.40, Brier 0.0469→0.0417 (baseline 0.0413). The model is unavailable before and after: FY2013–2019 have 0–6 exits (below the 20-exit floor) and Brier still trails baseline.
- Settlement labels: 7,681 billed household-years changed (before, no household was ever "settled" because `TotalBalance__c` is never ≤ 0). 6,934 had a non-dues line on the statement, 425 had multiple dues lines, 322 had a single dues line only — on such statements `TotalBalance__c` differs from the line balance 92% of the time, usually equals `TotalCharges__c`, and is never ≤ 0, i.e. the statement total is a gross-charges rollup rather than a net balance. Every shift traces to statement totals not being dues figures; none to the new derivation.
- Views payload: only `dues` settlement counts (and `built_at`) differ; every other Renewal & Engagement section is identical. Risk payload differs only in coverage/removed families/metrics. Verified at the payload level; the GUI was not launched. No privacy/audit code touched.
