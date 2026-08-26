# Membership Lifecycle Insights Design

Date: 2026-08-26

Status: approved in conversation; pending written review

Builds on: `2026-08-25-membership-insights-design.md`

Supersedes: that design's single-reason exit grouping, rule-only at-risk list, and v2 outline

## 1. Purpose

Insights must distinguish membership growth, relationship strength, addressable churn, and
unavoidable exits. An old historical fact must not label a current household as at risk. The
feature will also test whether stated joining reasons and observed participation can reveal the
"job" a household hired membership to do, while clearly labeling those signals as proxies rather
than causal knowledge of member intent.

The first release delivers three related capabilities from one consistent analytical foundation:

1. A household-by-fiscal-year mart for longitudinal analysis.
2. Aggregate membership, job, renewal, engagement, and exit views derived from that mart.
3. A predictive watch list only when rolling historical validation demonstrates useful signal.

The unit of analysis remains the Membership Household. Fiscal years run June 1 through May 31 and
are labeled by the year in which they end.

## 2. Success Criteria

- A single stale resignation or school-history reason cannot place a current household on the
  Watch List.
- Exit reporting separates Addressable Churn, Structural Exit, Conversion Loss, and
  Administrative or Unknown Exit without discarding secondary raw reasons.
- Staff can compare retention by Entry Job and by observed Relationship Anchor.
- Billing, enrollment, and committee sections clearly distinguish missing source coverage from
  member behavior.
- Every aggregate shown in the UI and PDF comes from the same household-year mart.
- Predictive household rankings remain hidden unless all coverage and validation gates pass.
- All calculations use the encrypted local mirror and expose their freshness.

## 3. Domain Model

Canonical terms are recorded in `CONTEXT.md`.

An Entry Job is stated evidence from `Join_Reason__c`. A Relationship Anchor is observed evidence
from dues billing, school enrollment, or committee service. These answer different questions and
must not be combined into a single "engagement" field.

Exit reasons are parsed into independent boolean labels. A reporting outcome is then assigned with
this precedence:

1. Structural Exit: moved, deceased, or elderly/ill.
2. Conversion Loss: an introductory or age-limited tier ended without conversion.
3. Addressable Churn: non-payment, financial hardship, no longer engaged, displeased, or joined
   another synagogue.
4. Administrative or Unknown Exit: other, administrative, or uncoded.

The precedence prevents an exit containing both "moved" and "non-payment" from becoming a model
target while retaining both raw labels for analysis. A current household's historical resignation
date or reason belongs to a previous Membership Spell and is not evidence that its current spell is
ending.

## 4. Source Capabilities

| Capability | Required synced objects | Purpose |
|---|---|---|
| Membership | `Account` | spells, Entry Jobs, tiers, current status, exits |
| Renewal | `BillingStatement__c`, `BillingStatementLine__c` | dues billing and settlement evidence |
| School | `Class_Enrolment__c` | confirmed Nursery School and Religious School anchors |
| Committee | `Committee_Membership__c` | current and historical committee anchors |

Membership views require `Account`. Each other capability is optional and independently gated.
Missing objects disable only their dependent sections and list the exact objects that need to be
selected and synced. Renewal insights require both billing objects because statement lines link to
the household through their parent statement.

No Contact-level death, pastoral, medical, or other sensitive fields are introduced. Deceased and
other exit classifications use the existing Account resignation reason only.

## 5. Architecture

The sync/profile flow triggers one transactional Insights rebuild:

```text
synced mirror tables
  -> normalized source rows
  -> _m_household
  -> _m_household_fy
  -> aggregate views and model dataset
```

Both marts rebuild in one database transaction. If normalization or insertion fails, rollback
leaves the previous valid marts intact. Metadata records the mart build time, newest source sync
time, available capabilities, source row counts, and unavailable reasons.

The existing `insights.rs` remains the command-facing facade. New code is isolated under focused
Insights submodules for normalization, mart construction, aggregate views, risk modeling, and
exports. This preserves the current command surface and avoids unrelated frontend or PDF
refactoring.

## 6. Household-Year Mart

`_m_household_fy` contains one row for each relevant Membership Household and fiscal year. It is
the only analytical input used by aggregate views and predictive evaluation.

Core fields include:

- household and fiscal year identifiers;
- active-at-year-end, joined, exited, tenure, cohort, and rejoin indicators;
- Entry Job flags and Entry Job count;
- independent exit-reason flags and primary Exit Outcome;
- tier and tier-conversion indicators;
- membership-dues billed amount, received amount, balance, due state, settlement state, and
  billing-coverage indicator;
- confirmed Nursery School and Religious School enrollment, withdrawn enrollment, and years since
  last confirmed Religious School enrollment;
- committee activity;
- Relationship Anchor flags and anchor count;
- field-level missingness and source-capability flags.

Account Membership Spells remain the official historical membership timeline. Renewal Evidence is
an observed anchor and consistency check, not automatic membership truth. A current household with
no dues line is reported as a coverage gap until billing completeness is proven; absence alone is
not churn evidence.

## 7. Derivation Rules

### Billing

Statement lines join to `BillingStatement__c`, then to `Account`. Membership dues include explicit
dues products and exclude security fees, gifts, school tuition, events, sales, and other non-dues
families. Classification is centralized and tested against observed product names and families.
Fiscal year is derived from the due date, with statement date as a documented fallback when due
date is absent.

For current descriptive views:

- `not due`: positive balance with a future due date;
- `settled`: balance is zero or negative;
- `partial`: positive received amount and positive balance after the due date;
- `outstanding`: no received amount and positive balance after the due date;
- `coverage missing`: active household with no qualifying dues line.

Historical balances and received amounts are current/final snapshots, not guaranteed as-of values.
Historical charts therefore label them "eventual settlement." They cannot become predictive
features unless source lineage proves that the value existed at the relevant fiscal-year cutoff.

### Enrollment

Only confirmed enrollment creates a Relationship Anchor. Nursery School and Religious School are
separate anchors. Withdrawn enrollment is retained as a separate outcome and never treated as
confirmed participation. Academic/fiscal year text is normalized to the app's fiscal-year
convention.

### Committee Membership

Historical activity uses valid `Member_From__c` and `Member_To__c` dates. Known far-future
placeholder end dates are treated as open-ended. `IsActive__c` is authoritative for current
committee activity; status remains supporting evidence.

### School Gap

Years since last confirmed Religious School enrollment is numeric and appears as a retention/churn
curve. Ending Religious School within the previous two completed fiscal years may be recent Risk
Evidence. Ending it earlier is historical context, not a named Watch List signal by itself.

## 8. Insights Experience

Insights uses four analytical tabs. The screen retains the existing rebuild, CSV, report mode, and
PDF behavior; PDF output composes all aggregate sections regardless of the selected tab.

### Overview

- Member households, joins, gross exits, Addressable Churn, and Structural Exits.
- Membership and flow trends.
- Exit Outcome composition.
- Cohort retention.

### Jobs

- Retention by Entry Job.
- Single-job versus multi-job retention.
- Exit Outcomes by Entry Job and tenure.
- Nursery School to Religious School progression.
- Addressable Churn by years since last confirmed Religious School enrollment.

The UI describes these as evidence about likely jobs, not a definitive statement of motivation.

### Renewal & Engagement

- Dues funnel: billed, not due, settled, partial, outstanding, and coverage missing.
- Current school and committee anchors.
- Retention by anchor type and anchor count.
- Clear unavailable states when dependent objects are unsynced.

### Risk

- Historical factor table with eligible households, Addressable Churn rate, baseline rate, and
  lift.
- Rolling model backtests with every test year shown, including failures.
- Calibration and ranking-quality metrics.
- A named Watch List loaded only on explicit request.

Household names are screen-only. PDFs contain aggregate findings and model-validation results, not
the named Watch List.

## 9. Predictive Validation

Each modeling row represents a household active at the end of fiscal year N. The target is an
Addressable Churn exit during fiscal year N+1. Structural Exits and Administrative or Unknown Exits
are excluded from training and evaluation. Conversion Loss is analyzed separately.

Candidate inputs are restricted to evidence available by the end of year N:

- tenure, cohort, and rejoin status;
- Entry Job flags and count;
- cutoff-valid dues billing and settlement evidence;
- Nursery School and Religious School enrollment and years since last Religious School;
- committee activity and Relationship Anchor count;
- explicit missingness indicators.

No current resignation date, resignation reason, post-cutoff payment state, or future enrollment
may enter a feature row. A feature-lineage test enforces the cutoff. Features without defensible
historical as-of values are omitted rather than approximated.

The model is regularized logistic regression implemented with an established Rust machine-learning
crate, not a custom optimizer. It is trained locally and evaluated with rolling fiscal-year
backtests. Coefficients and evidence are associations, not causal claims.

A model may score current households only when all gates pass:

- at least three completed rolling test years;
- at least 200 eligible households and 20 target exits in each test year;
- any optional feature source has at least 70% coverage in training and scoring data, with no more
  than a 15 percentage-point coverage shift;
- aggregate ROC-AUC is at least 0.65;
- top-decile lift is at least 2.0 times the baseline rate;
- Brier score is better than a constant baseline-rate prediction.

If an optional source fails coverage, the system removes that feature family, reruns the complete
backtest, and displays the validated feature set. If the remaining model fails, Risk shows factors
and validation results but no scores or named Watch List.

To appear on the named Watch List, a current household must:

1. be in the passing model's top risk decile; and
2. have at least two independent classes of current or recent Risk Evidence.

Multiple fields derived from one event count as one evidence class. Entry Job alone, a historical
resignation reason, missing billing coverage, or school ending more than two completed fiscal years
ago cannot satisfy the evidence requirement. Each row explains the observed evidence, comparison
baseline, model period, and confidence. The list is a review queue, not a prediction that a
household will resign.

## 10. Commands, Audit, and Export

The existing aggregate Insights command is extended with the four view payloads, source capability
metadata, mart/model freshness, and validation results. Aggregate reads are not audited because
they contain no household names.

Loading or exporting the named Watch List remains a separate audited command. CSV exports add the
new aggregate datasets and retain the existing restricted export directory and path validation.
PDF/report mode includes all aggregate tabs and excludes household names.

## 11. Failure Handling

- Missing Account data makes Insights unavailable with a specific sync instruction.
- Missing optional objects disable only Renewal, School, or Committee-dependent content.
- Failed mart rebuilds retain the prior mart and show both its age and the rebuild error.
- A model-training or validation failure never blocks aggregate Insights.
- Insufficient model quality produces an explicit "No validated household ranking" state rather
  than fallback rules masquerading as prediction.
- Current billing status and historical eventual settlement are labeled distinctly.

## 12. Testing

### Rust Unit Tests

- multi-label resignation parsing, outcome precedence, and current-spell isolation;
- fiscal-year boundaries, malformed dates, rejoin handling, and school-gap calculation;
- dues inclusion/exclusion and billing settlement categories;
- confirmed versus withdrawn enrollment;
- committee placeholder dates and current-status authority;
- evidence-class independence and two-signal Watch List rule.

### Mart Integration Tests

A synthetic encrypted mirror covers current, resigned, rejoined, structural, addressable,
conversion, and unknown households across multiple fiscal years. Tests rebuild both marts and
assert each aggregate from the household-year rows. Separate fixtures omit each optional source to
verify capability gates and transactional rollback.

### Model Tests

- a synthetic stable signal passes rolling validation and produces a Watch List;
- random/no-signal data fails and suppresses scores and household names;
- post-cutoff values trigger the leakage guard;
- optional-source coverage loss causes feature-family removal and complete revalidation;
- metrics include failed as well as passing test years.

### Frontend Tests

- typed command wrappers and capability states;
- four-tab navigation and report-mode composition;
- chart/table formatting, outcome labels, freshness, and unavailable-source prompts;
- named Watch List loads explicitly and is absent from PDF output;
- aggregate and named CSV export behavior.

### Real-Mirror Verification

After `BillingStatement__c` is selected and synced, compare mart row counts, exit totals, billing
coverage, enrollment totals, and committee totals with direct read-only aggregate queries. No
individual member records are printed during verification.

## 13. Out of Scope

- Contact-level or sensitive pastoral analysis;
- causal claims about why a household joined or left;
- automated outreach or Salesforce writes;
- editable model thresholds or user-authored risk rules;
- cloud training, third-party data transfer, or opaque generative-AI scoring;
- treating missing dues billing as proof that membership ended.
