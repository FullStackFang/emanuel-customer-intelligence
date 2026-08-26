## 1. OpenSpec Workflow

- [x] 1.1 Pin the OpenSpec CLI, add repository workflow instructions, and configure project context and artifact rules.
- [x] 1.2 Validate the complete OpenSpec change and remove the superseded standalone lifecycle design.

## 2. Baseline and Dependency Probe

- [ ] 2.1 Run and record the current Rust and frontend test baseline, including the existing stale-school-history exclusion test.
- [ ] 2.2 Add failing tests for multi-label Exit Outcomes, source capability gates, and household-year derivation.
- [ ] 2.3 Probe maintained Rust logistic-regression crates against the current MSVC and Cargo toolchain, select one, and add the pinned dependency.

## 3. Source Normalization

- [ ] 3.1 Implement and test multi-label resignation parsing, primary Exit Outcome precedence, and current-spell isolation.
- [ ] 3.2 Implement and test fiscal-year, membership-spell, rejoin, tenure, Entry Job, and school-gap normalization.
- [ ] 3.3 Implement and test membership-dues classification, statement-to-household joining, settlement states, and coverage missingness.
- [ ] 3.4 Implement and test confirmed/withdrawn enrollment normalization and Nursery School versus Religious School anchors.
- [ ] 3.5 Implement and test committee date normalization, far-future placeholders, and current activity authority.

## 4. Household-Year Mart

- [ ] 4.1 Add source capability and freshness metadata without changing the existing command-facing Insights facade.
- [ ] 4.2 Build `_m_household_fy` transactionally with `_m_household` and retain the prior valid marts on failure.
- [ ] 4.3 Add encrypted synthetic-mirror integration fixtures covering current, resigned, rejoined, structural, addressable, conversion, and unknown outcomes.
- [ ] 4.4 Verify all optional-source omission combinations and transactional rollback against the integration fixtures.

## 5. Aggregate Insights

- [ ] 5.1 Derive Overview membership flows, Exit Outcome composition, and cohort retention from `_m_household_fy`.
- [ ] 5.2 Derive Jobs retention, multi-job, outcome-by-tenure, Nursery-to-Religious-School, and school-gap views.
- [ ] 5.3 Derive Renewal & Engagement dues, school, committee, anchor-type, and anchor-count views with eventual-settlement labels.
- [ ] 5.4 Extend Rust payloads, TypeScript types, command wrappers, aggregate CSV exports, and freshness/unavailable metadata.

## 6. Predictive Validation

- [ ] 6.1 Build cutoff-safe fiscal-year feature rows and add tests that reject resignation, future enrollment, post-cutoff payment, and unproven historical snapshot leakage.
- [ ] 6.2 Implement regularized logistic regression and rolling test-year evaluation with sample-size, coverage, ROC-AUC, lift, and Brier gates.
- [ ] 6.3 Implement optional feature-family removal followed by complete revalidation when coverage or drift gates fail.
- [ ] 6.4 Add synthetic stable-signal and no-signal tests proving that rankings appear only for a passing model.
- [ ] 6.5 Implement the top-decile plus two-independent-evidence Watch List gate, including explicit stale Religious School and one-event tests.
- [ ] 6.6 Add audited named Watch List load/export with evidence, comparison baseline, model period, and confidence.

## 7. Insights Interface

- [ ] 7.1 Add Overview, Jobs, Renewal & Engagement, and Risk tabs using the existing design system and stable responsive layouts.
- [ ] 7.2 Add aggregate charts and table twins for the new views, including source-unavailable, stale, rebuild-failed, and no-validated-ranking states.
- [ ] 7.3 Load household names only after explicit Watch List request and render evidence without causal or deterministic language.
- [ ] 7.4 Compose every aggregate section in report/PDF mode while excluding named households, and extend aggregate CSV selection.
- [ ] 7.5 Add frontend tests for API contracts, tab behavior, formatting, unavailable states, explicit named loading, and PDF privacy.

## 8. Verification and Rollout

- [ ] 8.1 Run formatting, type checking, frontend tests, Rust tests, and production builds; resolve only regressions attributable to this change.
- [ ] 8.2 Select and sync `BillingStatement__c`, rebuild Insights, and compare aggregate mart totals with read-only source queries without printing member records.
- [ ] 8.3 Verify desktop and mobile Insights rendering, report/PDF output, named-list auditing, exports, and failure states in the running app.
- [ ] 8.4 Record model coverage and every rolling backtest result; confirm household rankings remain absent unless all specified gates pass.
