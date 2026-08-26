//! Historically validated Addressable Churn prediction.
//!
//! Each model row represents a Membership Household active at the end of fiscal year N;
//! the target is Addressable Churn during N+1. Only evidence demonstrably available by
//! the end of N may become a feature — current resignation fields, future enrollment,
//! post-cutoff payments, and final balance/settlement snapshots without cutoff lineage
//! are rejected. Structural, Conversion, and Administrative exits are excluded from the
//! population rather than labeled as retention outcomes.

use crate::insights::{member_in, Hh, HhFy, SourceCapability, INTRO_TIERS};
use serde::Serialize;

/// Primary Exit Outcome that is the prediction target.
pub const ADDRESSABLE: &str = "Addressable Churn";

/// Fixed feature order. Base features are always present; the optional families
/// (renewal, school, committee) are neutralized to 0 when their source is unavailable or
/// carried no data for the row's fiscal year, and reported through `FeatureRow`'s
/// per-family observed flags.
pub const FEATURE_NAMES: [&str; 9] = [
    "tenure_years",                 // base
    "entry_job_count",              // base
    "intro_tier",                   // base
    "religious_school_ended",       // base (from last attended year, never a live snapshot)
    "years_since_religious_school", // base
    "dues_billed",                  // renewal family
    "dues_coverage_missing",        // renewal family
    "school_anchor",                // school family
    "committee_anchor",             // committee family
];

/// Column indices of each optional feature family within `FEATURE_NAMES`.
pub const RENEWAL_COLUMNS: [usize; 2] = [5, 6];
pub const SCHOOL_COLUMNS: [usize; 1] = [7];
pub const COMMITTEE_COLUMNS: [usize; 1] = [8];

/// One leakage-controlled training/scoring row.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureRow {
    pub account_id: String,
    pub fy_n: i32,
    /// 1 = Addressable Churn in N+1, 0 = retained through N+1.
    pub label: u8,
    pub features: [f64; 9],
    /// Whether each optional family's source was available for the run (reporting only;
    /// coverage is gated on the per-year `*_observed` flags below).
    pub has_renewal: bool,
    pub has_school: bool,
    pub has_committee: bool,
    /// Whether each optional family's source actually carried data for this row's fiscal
    /// year. A year with no source rows is uncovered for that family, even when the
    /// source is available, so the coverage gate can see it.
    pub renewal_observed: bool,
    pub school_observed: bool,
    pub committee_observed: bool,
}

fn cap_available(caps: &[SourceCapability], key: &str) -> bool {
    caps.iter().any(|c| c.key == key && c.available)
}

/// Population + target for household `hh` at cutoff N. `None` excludes the household:
/// it was not active at the end of N, or it exited N+1 for a non-Addressable reason,
/// or its N+1 state cannot be confirmed.
fn label_at(hh: &Hh, n: i32) -> Option<u8> {
    if !member_in(hh, n) {
        return None;
    }
    if hh.resign_fy == Some(n + 1) {
        // A resignation in N+1 is the only place a future exit may be read, and only to
        // form the label — never as a feature.
        return (hh.resign_reason_group == ADDRESSABLE).then_some(1);
    }
    // Still active at the end of N+1 is a retained (negative) outcome. Any other case
    // (e.g. an undated resignation) cannot be confirmed as Addressable and is excluded.
    member_in(hh, n + 1).then_some(0)
}

/// Build cutoff-safe feature rows for cutoff year `n`. Features read only fiscal year
/// `n` and static join-time facts; nothing after the cutoff enters the vector.
pub fn feature_rows(
    hh: &[Hh],
    years: &[HhFy],
    n: i32,
    caps: &[SourceCapability],
) -> Vec<FeatureRow> {
    hh.iter()
        .filter_map(|household| {
            let label = label_at(household, n)?;
            build_feature_row(household, years, n, label, caps)
        })
        .collect()
}

/// Feature rows for households active at the end of `n`, without a label — used to score
/// current households whose N+1 outcome is not yet known. The `label` field is 0.
pub fn scoring_rows(
    hh: &[Hh],
    years: &[HhFy],
    n: i32,
    caps: &[SourceCapability],
) -> Vec<FeatureRow> {
    hh.iter()
        .filter_map(|household| {
            if !member_in(household, n) {
                return None;
            }
            build_feature_row(household, years, n, 0, caps)
        })
        .collect()
}

/// Compute one cutoff-safe feature vector. Reads only fiscal year `n` and static
/// join-time facts; nothing after the cutoff enters the vector.
fn build_feature_row(
    household: &Hh,
    years: &[HhFy],
    n: i32,
    label: u8,
    caps: &[SourceCapability],
) -> Option<FeatureRow> {
    let join_fy = household.join_fy?;
    let has_renewal = cap_available(caps, "renewal");
    let has_school = cap_available(caps, "school");
    let has_committee = cap_available(caps, "committee");
    let at_n = years
        .iter()
        .find(|r| r.account_id == household.account_id && r.fy == n);

    let tenure = (n - join_fy + 1).max(0) as f64;
    let entry_jobs = household.ch.iter().filter(|f| **f).count() as f64;
    let intro = household
        .tier
        .as_deref()
        .is_some_and(|t| INTRO_TIERS.contains(&t)) as i32 as f64;
    // Religious School status uses the last attended year (known by the cutoff), never
    // the live active-student count, which is a post-cutoff snapshot.
    let last_rs = household.last_rs_year.filter(|y| *y <= n);
    let rs_ended = last_rs.is_some_and(|y| n - y >= 1) as i32 as f64;
    let years_since_rs = last_rs.map(|y| (n - y).max(0) as f64).unwrap_or(0.0);

    // A family is observed for this row only when its source is available AND carried
    // data for fiscal year N. An unobserved year neutralizes the family to 0 — it is
    // uncovered, not a zero anchor and not "coverage missing".
    let renewal_observed = has_renewal && at_n.is_some_and(|r| r.renewal_observed);
    let school_observed = has_school && at_n.is_some_and(|r| r.school_observed);
    let committee_observed = has_committee && at_n.is_some_and(|r| r.committee_observed);

    // Optional-family anchors observed during fiscal year N (available by its end).
    // dues_settlement is deliberately NOT a feature: it is an eventual state.
    let dues_billed = (renewal_observed && at_n.is_some_and(|r| r.anchor_dues)) as i32 as f64;
    let dues_missing =
        (renewal_observed && at_n.is_some_and(|r| r.dues_coverage_missing)) as i32 as f64;
    let school_anchor = (school_observed
        && at_n.is_some_and(|r| r.anchor_nursery || r.anchor_religious))
        as i32 as f64;
    let committee_anchor =
        (committee_observed && at_n.is_some_and(|r| r.anchor_committee)) as i32 as f64;

    Some(FeatureRow {
        account_id: household.account_id.clone(),
        fy_n: n,
        label,
        features: [
            tenure,
            entry_jobs,
            intro,
            rs_ended,
            years_since_rs,
            dues_billed,
            dues_missing,
            school_anchor,
            committee_anchor,
        ],
        has_renewal,
        has_school,
        has_committee,
        renewal_observed,
        school_observed,
        committee_observed,
    })
}

// ── Model and rolling validation ────────────────────────────────────────────

use smartcore::linalg::basic::arrays::Array;
use smartcore::linalg::basic::matrix::DenseMatrix;
use smartcore::linear::logistic_regression::{LogisticRegression, LogisticRegressionParameters};

/// Validation gates (design §"Use validation-gated regularized logistic regression").
pub const MIN_TEST_YEARS: usize = 3;
pub const MIN_HOUSEHOLDS: i64 = 200;
pub const MIN_EXITS: i64 = 20;
pub const MIN_AUC: f64 = 0.65;
pub const MIN_LIFT: f64 = 2.0;
/// L2 regularization strength for the logistic regression.
pub const DEFAULT_ALPHA: f64 = 1.0;

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// A fitted regularized logistic regression over a fixed set of feature columns.
pub struct Model {
    pub columns: Vec<usize>,
    pub coef: Vec<f64>,
    pub intercept: f64,
}

impl Model {
    /// P(Addressable Churn) for one row.
    pub fn score(&self, row: &FeatureRow) -> f64 {
        let z = self.intercept
            + self
                .columns
                .iter()
                .zip(&self.coef)
                .map(|(&c, w)| row.features[c] * w)
                .sum::<f64>();
        sigmoid(z)
    }
}

/// Fit a regularized logistic regression on `rows` using only `columns`. Returns None
/// when a class is absent or the solver cannot fit — smartcore owns the optimization.
pub fn fit(rows: &[&FeatureRow], columns: &[usize], alpha: f64) -> Option<Model> {
    if columns.is_empty() || rows.len() < 2 {
        return None;
    }
    let y: Vec<i64> = rows.iter().map(|r| r.label as i64).collect();
    if !y.iter().any(|&v| v == 0) || !y.iter().any(|&v| v == 1) {
        return None;
    }
    let x_data: Vec<Vec<f64>> = rows
        .iter()
        .map(|r| columns.iter().map(|&c| r.features[c]).collect())
        .collect();
    let x = DenseMatrix::from_2d_vec(&x_data).ok()?;
    let params = LogisticRegressionParameters::default().with_alpha(alpha);
    let lr = LogisticRegression::fit(&x, &y, params).ok()?;
    let coef = (0..columns.len())
        .map(|j| *lr.coefficients().get((0, j)))
        .collect();
    let intercept = *lr.intercept().get((0, 0));
    Some(Model {
        columns: columns.to_vec(),
        coef,
        intercept,
    })
}

/// ROC-AUC via the tie-aware Mann–Whitney U statistic. Returns 0.5 with no contrast.
pub fn roc_auc(scored: &[(f64, u8)]) -> f64 {
    let pos = scored.iter().filter(|(_, y)| *y == 1).count();
    let neg = scored.len() - pos;
    if pos == 0 || neg == 0 {
        return 0.5;
    }
    let mut order: Vec<usize> = (0..scored.len()).collect();
    order.sort_by(|&a, &b| {
        scored[a]
            .0
            .partial_cmp(&scored[b].0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ranks = vec![0.0; scored.len()];
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len() && scored[order[j + 1]].0 == scored[order[i]].0 {
            j += 1;
        }
        let avg_rank = ((i + 1) + (j + 1)) as f64 / 2.0; // ranks are 1-based
        for slot in &order[i..=j] {
            ranks[*slot] = avg_rank;
        }
        i = j + 1;
    }
    let sum_pos: f64 = scored
        .iter()
        .zip(&ranks)
        .filter(|((_, y), _)| *y == 1)
        .map(|(_, r)| *r)
        .sum();
    let u = sum_pos - (pos * (pos + 1)) as f64 / 2.0;
    u / (pos as f64 * neg as f64)
}

/// Churn rate in the highest-scoring decile divided by the base rate.
pub fn top_decile_lift(scored: &[(f64, u8)]) -> f64 {
    if scored.is_empty() {
        return 0.0;
    }
    let base = scored.iter().filter(|(_, y)| *y == 1).count() as f64 / scored.len() as f64;
    if base == 0.0 {
        return 0.0;
    }
    let mut s = scored.to_vec();
    s.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let k = ((scored.len() as f64 * 0.1).ceil() as usize).max(1);
    let top_rate = s[..k].iter().filter(|(_, y)| *y == 1).count() as f64 / k as f64;
    top_rate / base
}

/// Mean squared error of the probabilities against outcomes.
pub fn brier(scored: &[(f64, u8)]) -> f64 {
    if scored.is_empty() {
        return 0.0;
    }
    scored
        .iter()
        .map(|(p, y)| (p - *y as f64).powi(2))
        .sum::<f64>()
        / scored.len() as f64
}

/// Brier score of the constant base-rate predictor, the bar calibration must beat.
fn baseline_brier(scored: &[(f64, u8)]) -> f64 {
    if scored.is_empty() {
        return 0.0;
    }
    let base = scored.iter().filter(|(_, y)| *y == 1).count() as f64 / scored.len() as f64;
    scored
        .iter()
        .map(|(_, y)| (base - *y as f64).powi(2))
        .sum::<f64>()
        / scored.len() as f64
}

/// One rolling test year.
#[derive(Debug, Clone)]
pub struct YearResult {
    pub test_fy: i32,
    pub households: i64,
    pub exits: i64,
    pub sufficient: bool,
}

/// Outcome of a rolling backtest over a feature-column set.
#[derive(Debug, Clone)]
pub struct Validation {
    pub columns: Vec<usize>,
    pub years: Vec<YearResult>,
    pub roc_auc: f64,
    pub top_decile_lift: f64,
    pub brier: f64,
    pub baseline_brier: f64,
    pub passed: bool,
    pub failures: Vec<String>,
}

/// Rolling fiscal-year backtest: for each candidate test year with prior training
/// history, train on earlier years and score that year. Aggregate metrics pool every
/// sufficient test year's scored predictions. All gates must pass for `passed`.
pub fn rolling_validation(rows: &[FeatureRow], columns: &[usize], alpha: f64) -> Validation {
    let mut test_years: Vec<i32> = rows.iter().map(|r| r.fy_n).collect();
    test_years.sort_unstable();
    test_years.dedup();

    let mut years = Vec::new();
    let mut pooled: Vec<(f64, u8)> = Vec::new();
    for &t in &test_years {
        let train: Vec<&FeatureRow> = rows.iter().filter(|r| r.fy_n < t).collect();
        if train.is_empty() {
            continue; // the earliest year has no history and is not a test year
        }
        let test: Vec<&FeatureRow> = rows.iter().filter(|r| r.fy_n == t).collect();
        let households = test.len() as i64;
        let exits = test.iter().filter(|r| r.label == 1).count() as i64;
        let sufficient = households >= MIN_HOUSEHOLDS && exits >= MIN_EXITS;
        years.push(YearResult {
            test_fy: t,
            households,
            exits,
            sufficient,
        });
        if sufficient {
            if let Some(model) = fit(&train, columns, alpha) {
                for r in &test {
                    pooled.push((model.score(r), r.label));
                }
            }
        }
    }

    let completed = years.iter().filter(|y| y.sufficient).count();
    let roc = roc_auc(&pooled);
    let lift = top_decile_lift(&pooled);
    let br = brier(&pooled);
    let base_br = baseline_brier(&pooled);

    let mut failures = Vec::new();
    if completed < MIN_TEST_YEARS {
        failures.push(format!(
            "Only {completed} test year(s) met the sample floor; {MIN_TEST_YEARS} required"
        ));
    }
    for y in years.iter().filter(|y| !y.sufficient) {
        failures.push(format!(
            "FY{} had {} households and {} exits (need {} and {})",
            y.test_fy, y.households, y.exits, MIN_HOUSEHOLDS, MIN_EXITS
        ));
    }
    if completed >= MIN_TEST_YEARS {
        if roc < MIN_AUC {
            failures.push(format!("ROC-AUC {roc:.3} below {MIN_AUC}"));
        }
        if lift < MIN_LIFT {
            failures.push(format!("Top-decile lift {lift:.2} below {MIN_LIFT}"));
        }
        if br >= base_br {
            failures.push(format!(
                "Brier {br:.4} not better than baseline {base_br:.4}"
            ));
        }
    }
    Validation {
        columns: columns.to_vec(),
        years,
        roc_auc: roc,
        top_decile_lift: lift,
        brier: br,
        baseline_brier: base_br,
        passed: failures.is_empty(),
        failures,
    }
}

// ── Optional-family coverage and revalidation ───────────────────────────────

/// Base features are always eligible; optional families are gated on coverage.
pub const BASE_COLUMNS: [usize; 5] = [0, 1, 2, 3, 4];
pub const MIN_COVERAGE: f64 = 0.70;
pub const MAX_DRIFT: f64 = 0.15;

/// The optional feature families and the columns each contributes.
fn optional_families() -> [(&'static str, &'static [usize]); 3] {
    [
        ("renewal", &RENEWAL_COLUMNS),
        ("school", &SCHOOL_COLUMNS),
        ("committee", &COMMITTEE_COLUMNS),
    ]
}

/// Whether a row carries observed data for an optional family. Every model feature is a
/// leakage-safe anchor indicator, so a family is "covered" exactly when its source
/// carried data for that row's fiscal year; there is no partial as-of measurement to be
/// missing. Availability alone (`has_*`) is not coverage: a year the source has no rows
/// for is uncovered, which lets the gate drop a family whose data starts late.
fn family_observed(row: &FeatureRow, family: &str) -> bool {
    match family {
        "renewal" => row.renewal_observed,
        "school" => row.school_observed,
        "committee" => row.committee_observed,
        _ => true,
    }
}

fn coverage(rows: &[&FeatureRow], family: &str) -> f64 {
    if rows.is_empty() {
        return 0.0;
    }
    rows.iter().filter(|r| family_observed(r, family)).count() as f64 / rows.len() as f64
}

/// Coverage of one optional family in training and scoring splits.
#[derive(Debug, Clone)]
pub struct FamilyCoverage {
    pub family: String,
    pub train: f64,
    pub score: f64,
    pub kept: bool,
}

/// The result of validating with coverage-driven family removal.
#[derive(Debug, Clone)]
pub struct RiskModel {
    pub validation: Validation,
    pub coverage: Vec<FamilyCoverage>,
    pub removed_families: Vec<String>,
}

/// Evaluate the churn model, removing any optional feature family that fails the coverage
/// or drift gate and then rerunning the complete rolling backtest on the remaining
/// features. A family is removed when its training or scoring coverage is below
/// `MIN_COVERAGE` or the two differ by more than `MAX_DRIFT`.
pub fn evaluate(rows: &[FeatureRow], alpha: f64) -> RiskModel {
    // Split coverage on the most recent scored year vs. its training history.
    let latest = rows.iter().map(|r| r.fy_n).max();
    let (train, score): (Vec<&FeatureRow>, Vec<&FeatureRow>) = match latest {
        Some(fy) if rows.iter().any(|r| r.fy_n < fy) => (
            rows.iter().filter(|r| r.fy_n < fy).collect(),
            rows.iter().filter(|r| r.fy_n == fy).collect(),
        ),
        // Not enough history to split; compare a family against itself.
        _ => (rows.iter().collect(), rows.iter().collect()),
    };

    let mut removed: Vec<String> = Vec::new();
    let mut coverage_report: Vec<FamilyCoverage> = Vec::new();
    loop {
        coverage_report.clear();
        let mut to_remove: Option<String> = None;
        for (family, _) in optional_families()
            .iter()
            .filter(|(f, _)| !removed.iter().any(|r| r == f))
        {
            let train_cov = coverage(&train, family);
            let score_cov = coverage(&score, family);
            let kept = train_cov >= MIN_COVERAGE
                && score_cov >= MIN_COVERAGE
                && (train_cov - score_cov).abs() <= MAX_DRIFT;
            coverage_report.push(FamilyCoverage {
                family: (*family).to_string(),
                train: train_cov,
                score: score_cov,
                kept,
            });
            if !kept {
                to_remove = Some((*family).to_string());
            }
        }
        match to_remove {
            Some(family) => removed.push(family),
            None => break,
        }
    }

    let mut columns = BASE_COLUMNS.to_vec();
    for (family, cols) in optional_families() {
        if !removed.iter().any(|r| r == family) {
            columns.extend_from_slice(cols);
        }
    }
    columns.sort_unstable();

    RiskModel {
        validation: rolling_validation(rows, &columns, alpha),
        coverage: coverage_report,
        removed_families: removed,
    }
}

// ── Evidence-gated named Watch List ─────────────────────────────────────────

/// One independent class of current or recent Risk Evidence for a household.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub class: String,
    pub detail: String,
}

fn any_anchor_source(caps: &[SourceCapability]) -> bool {
    ["renewal", "school", "committee"]
        .iter()
        .any(|k| cap_available(caps, k))
}

/// Independent classes of current or recent Risk Evidence for a current household.
/// Deliberately excluded from ever counting: an Entry Job on its own, merely missing
/// billing coverage, a prior spell's resignation reason, and a Religious School that
/// ended more than two completed fiscal years ago. Fields from one event yield one class.
pub fn evidence_classes(
    h: &Hh,
    years: &[HhFy],
    cur: i32,
    caps: &[SourceCapability],
) -> Vec<Evidence> {
    let mut out = Vec::new();
    // Recent Religious School end: within the last two completed fiscal years. A stale
    // end (older than that) is explicitly not current risk evidence.
    if h.rs_family && h.active_rs_students == 0 {
        if let Some(y) = h.last_rs_year.filter(|y| *y >= cur - 2 && *y <= cur) {
            out.push(Evidence {
                class: "recent_religious_school_end".into(),
                detail: format!("Religious School last attended FY{y}"),
            });
        }
    }
    // Introductory tier that has aged past its conversion window.
    if h.tier.as_deref().is_some_and(|t| INTRO_TIERS.contains(&t)) {
        if let Some(j) = h.join_fy.filter(|j| cur - j >= 2) {
            out.push(Evidence {
                class: "intro_tier_aging".into(),
                detail: format!("{} since FY{j}", h.tier.clone().unwrap_or_default()),
            });
        }
    }
    // A brand-new household in its first completed year.
    if h.join_fy == Some(cur - 1) {
        out.push(Evidence {
            class: "new_household".into(),
            detail: "Joined in the last completed fiscal year".into(),
        });
    }
    // Observed engagement loss: held a Relationship Anchor recently but none this year.
    // This is a positive-anchor drop, not merely absent billing coverage.
    if any_anchor_source(caps) {
        let held_recent = years.iter().any(|r| {
            r.account_id == h.account_id && r.fy >= cur - 3 && r.fy < cur && r.anchor_count() > 0
        });
        let held_now = years
            .iter()
            .any(|r| r.account_id == h.account_id && r.fy == cur && r.anchor_count() > 0);
        if held_recent && !held_now {
            out.push(Evidence {
                class: "lost_engagement_anchor".into(),
                detail: "Held a relationship anchor recently but not this year".into(),
            });
        }
    }
    out
}

/// A scored current household considered for the Watch List.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub account_id: String,
    pub name: String,
    pub score: f64,
    pub evidence: Vec<Evidence>,
}

/// A household that qualified for the named Watch List.
#[derive(Debug, Clone)]
pub struct WatchRow {
    pub account_id: String,
    pub name: String,
    pub score: f64,
    pub evidence: Vec<Evidence>,
}

/// Apply the two-part gate: a household must be in the top risk decile AND carry at
/// least two independent classes of Risk Evidence. Favors a small, defensible queue.
pub fn watch_list(candidates: &[Candidate]) -> Vec<WatchRow> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut scores: Vec<f64> = candidates.iter().map(|c| c.score).collect();
    scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let k = ((candidates.len() as f64 * 0.1).ceil() as usize).max(1);
    let threshold = scores[k - 1]; // the k-th highest score bounds the top decile
    let mut rows: Vec<WatchRow> = candidates
        .iter()
        .filter(|c| c.score >= threshold)
        .filter(|c| distinct_classes(&c.evidence) >= 2)
        .map(|c| WatchRow {
            account_id: c.account_id.clone(),
            name: c.name.clone(),
            score: c.score,
            evidence: c.evidence.clone(),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.account_id.cmp(&b.account_id))
    });
    rows
}

fn distinct_classes(evidence: &[Evidence]) -> usize {
    let mut classes: Vec<&str> = evidence.iter().map(|e| e.class.as_str()).collect();
    classes.sort_unstable();
    classes.dedup();
    classes.len()
}

/// The full named-risk result: either a validated Watch List or an explicit reason it is
/// unavailable. Aggregate Risk content remains available regardless.
#[derive(Debug, Clone)]
pub struct WatchList {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub model_first_fy: Option<i32>,
    pub model_last_fy: Option<i32>,
    pub baseline_rate: f64,
    pub rows: Vec<WatchRow>,
}

/// Build the named Watch List. Rankings appear only when the model passes validation;
/// otherwise the list is unavailable with the failing reason, and aggregate Risk content
/// (returned separately) still stands.
pub fn build_watch_list(
    labeled: &[FeatureRow],
    hh: &[Hh],
    years: &[HhFy],
    caps: &[SourceCapability],
    cur: i32,
    alpha: f64,
) -> (RiskModel, WatchList) {
    let model = evaluate(labeled, alpha);
    let (first, last) = (
        model.validation.years.iter().map(|y| y.test_fy).min(),
        model.validation.years.iter().map(|y| y.test_fy).max(),
    );
    let baseline_rate = if labeled.is_empty() {
        0.0
    } else {
        labeled.iter().filter(|r| r.label == 1).count() as f64 / labeled.len() as f64
    };
    if !model.validation.passed {
        let unavailable = WatchList {
            available: false,
            unavailable_reason: Some(model.validation.failures.join("; ")),
            model_first_fy: first,
            model_last_fy: last,
            baseline_rate,
            rows: Vec::new(),
        };
        return (model, unavailable);
    }
    // Refit on all labeled history using the validated feature columns, then score
    // current households at the last completed fiscal year.
    let refs: Vec<&FeatureRow> = labeled.iter().collect();
    let Some(fitted) = fit(&refs, &model.validation.columns, alpha) else {
        let unavailable = WatchList {
            available: false,
            unavailable_reason: Some("Final model could not be fit".into()),
            model_first_fy: first,
            model_last_fy: last,
            baseline_rate,
            rows: Vec::new(),
        };
        return (model, unavailable);
    };
    let score_fy = cur - 1;
    let candidates: Vec<Candidate> = scoring_rows(hh, years, score_fy, caps)
        .iter()
        .filter_map(|row| {
            let h = hh.iter().find(|h| h.account_id == row.account_id)?;
            if !h.is_current {
                return None; // the named list is current households only
            }
            Some(Candidate {
                account_id: h.account_id.clone(),
                name: h.name.clone().unwrap_or_default(),
                score: fitted.score(row),
                evidence: evidence_classes(h, years, cur, caps),
            })
        })
        .collect();
    let list = WatchList {
        available: true,
        unavailable_reason: None,
        model_first_fy: first,
        model_last_fy: last,
        baseline_rate,
        rows: watch_list(&candidates),
    };
    (model, list)
}

// ── Store-facing analysis and serializable payloads (task 6.6) ──────────────

/// Earliest cutoff year to build model rows from; older years lack the mirrored fields.
pub const FIRST_CUTOFF_FY: i32 = 2012;

/// Build every labeled feature row across trainable cutoff years, then produce the
/// validated model and named Watch List. Cutoffs run through `cur - 2` so each N+1 target
/// year is completed; current households are scored at `cur - 1`.
pub fn analyze(
    hh: &[Hh],
    years: &[HhFy],
    caps: &[SourceCapability],
    cur: i32,
    alpha: f64,
) -> (RiskModel, WatchList) {
    let mut labeled = Vec::new();
    for n in FIRST_CUTOFF_FY..=(cur - 2) {
        labeled.extend(feature_rows(hh, years, n, caps));
    }
    build_watch_list(&labeled, hh, years, caps, cur, alpha)
}

#[derive(Serialize, Debug, Clone)]
pub struct YearSummary {
    pub test_fy: i32,
    pub households: i64,
    pub exits: i64,
    pub sufficient: bool,
}
#[derive(Serialize, Debug, Clone)]
pub struct FamilyCoverageView {
    pub family: String,
    pub train: f64,
    pub score: f64,
    pub kept: bool,
}
/// Aggregate Risk view — validation results and backtests, never any household name.
#[derive(Serialize, Debug, Clone)]
pub struct RiskSummary {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub roc_auc: f64,
    pub top_decile_lift: f64,
    pub brier: f64,
    pub baseline_brier: f64,
    pub years: Vec<YearSummary>,
    pub coverage: Vec<FamilyCoverageView>,
    pub removed_families: Vec<String>,
    pub model_first_fy: Option<i32>,
    pub model_last_fy: Option<i32>,
    pub watch_list_count: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct EvidenceView {
    pub class: String,
    pub detail: String,
}
#[derive(Serialize, Debug, Clone)]
pub struct WatchRowView {
    pub account_id: String,
    pub name: String,
    pub score: f64,
    pub evidence: Vec<EvidenceView>,
}
/// Named Watch List view — loaded only on explicit, audited request.
#[derive(Serialize, Debug, Clone)]
pub struct WatchListView {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub model_first_fy: Option<i32>,
    pub model_last_fy: Option<i32>,
    pub baseline_rate: f64,
    pub confidence: f64,
    pub rows: Vec<WatchRowView>,
}

/// Assemble the name-free aggregate Risk summary.
pub fn risk_summary(model: &RiskModel, list: &WatchList) -> RiskSummary {
    RiskSummary {
        available: model.validation.passed,
        unavailable_reason: list.unavailable_reason.clone(),
        roc_auc: model.validation.roc_auc,
        top_decile_lift: model.validation.top_decile_lift,
        brier: model.validation.brier,
        baseline_brier: model.validation.baseline_brier,
        years: model
            .validation
            .years
            .iter()
            .map(|y| YearSummary {
                test_fy: y.test_fy,
                households: y.households,
                exits: y.exits,
                sufficient: y.sufficient,
            })
            .collect(),
        coverage: model
            .coverage
            .iter()
            .map(|c| FamilyCoverageView {
                family: c.family.clone(),
                train: c.train,
                score: c.score,
                kept: c.kept,
            })
            .collect(),
        removed_families: model.removed_families.clone(),
        model_first_fy: list.model_first_fy,
        model_last_fy: list.model_last_fy,
        watch_list_count: list.rows.len() as i64,
    }
}

/// Assemble the named Watch List view (evidence, comparison baseline, model period,
/// confidence). Confidence is the validated aggregate ROC-AUC.
pub fn watch_list_view(model: &RiskModel, list: &WatchList) -> WatchListView {
    WatchListView {
        available: list.available,
        unavailable_reason: list.unavailable_reason.clone(),
        model_first_fy: list.model_first_fy,
        model_last_fy: list.model_last_fy,
        baseline_rate: list.baseline_rate,
        confidence: model.validation.roc_auc,
        rows: list
            .rows
            .iter()
            .map(|r| WatchRowView {
                account_id: r.account_id.clone(),
                name: r.name.clone(),
                score: (r.score * 1000.0).round() / 1000.0,
                evidence: r
                    .evidence
                    .iter()
                    .map(|e| EvidenceView {
                        class: e.class.clone(),
                        detail: e.detail.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV of the named Watch List. Evidence classes are joined; no raw feature values.
pub fn watch_list_csv(view: &WatchListView) -> String {
    let mut out = String::from("account_id,name,score,evidence\n");
    for row in &view.rows {
        let classes = row
            .evidence
            .iter()
            .map(|e| e.class.as_str())
            .collect::<Vec<_>>()
            .join(";");
        out.push_str(&format!(
            "{},{},{},{}\n",
            csv_cell(&row.account_id),
            csv_cell(&row.name),
            row.score,
            csv_cell(&classes)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hh(id: &str, current: bool, join: i32, resign: Option<i32>, outcome: &str) -> Hh {
        Hh {
            account_id: id.into(),
            is_current: current,
            is_resigned: !current,
            join_fy: Some(join),
            cohort_fy: Some(join),
            resign_fy: if current { None } else { resign },
            resigned_unknown_date: !current && resign.is_none(),
            resign_reason_group: outcome.into(),
            ..Default::default()
        }
    }

    /// An active household-year in a fiscal year every optional source has data for.
    fn year(id: &str, fy: i32) -> HhFy {
        HhFy {
            account_id: id.into(),
            fy,
            active_end_of_fy: true,
            renewal_observed: true,
            school_observed: true,
            committee_observed: true,
            ..Default::default()
        }
    }

    fn caps(keys: &[&str]) -> Vec<SourceCapability> {
        keys.iter()
            .map(|k| SourceCapability {
                key: (*k).to_string(),
                available: true,
                required_objects: vec![],
                mirrored_columns: vec![],
                last_synced_at: None,
                unavailable_reason: None,
            })
            .collect()
    }

    #[test]
    fn population_excludes_non_addressable_exits_and_keeps_addressable() {
        let n = 2024;
        let households = vec![
            hh("churn", false, 2015, Some(2025), ADDRESSABLE), // label 1
            hh("moved", false, 2015, Some(2025), "Structural Exit"), // excluded
            hh("stay", true, 2015, None, "(not coded)"),       // label 0
            hh("early", true, 2026, None, "(not coded)"),      // not active at end of N
        ];
        let years: Vec<HhFy> = households
            .iter()
            .flat_map(|h| [year(&h.account_id, n), year(&h.account_id, n + 1)])
            .collect();
        let rows = feature_rows(&households, &years, n, &caps(&[]));
        let ids: Vec<&str> = rows.iter().map(|r| r.account_id.as_str()).collect();
        assert!(ids.contains(&"churn") && ids.contains(&"stay"));
        assert!(
            !ids.contains(&"moved"),
            "structural exit excluded, not negative"
        );
        assert!(!ids.contains(&"early"), "must be active at end of N");
        let label = |id: &str| rows.iter().find(|r| r.account_id == id).unwrap().label;
        assert_eq!(label("churn"), 1);
        assert_eq!(label("stay"), 0);
    }

    #[test]
    fn future_resignation_does_not_leak_into_features() {
        // Two identical households; one resigns (Addressable) in N+1, one stays. Their
        // feature vectors must be identical — the label differs, the features do not.
        let n = 2024;
        let churn = hh("churn", false, 2015, Some(2025), ADDRESSABLE);
        let stay = hh("stay", true, 2015, None, "(not coded)");
        let years: Vec<HhFy> = [&churn, &stay]
            .iter()
            .flat_map(|h| [year(&h.account_id, n), year(&h.account_id, n + 1)])
            .collect();
        let rows = feature_rows(&[churn, stay], &years, n, &caps(&[]));
        let f = |id: &str| rows.iter().find(|r| r.account_id == id).unwrap().features;
        assert_eq!(f("churn"), f("stay"), "features are cutoff-safe");
    }

    #[test]
    fn post_cutoff_anchors_are_rejected() {
        // Dues billed, school enrolment, and committee service that appear only in N+1
        // must not raise the household's features at cutoff N.
        let n = 2024;
        let household = hh("late", true, 2015, None, "(not coded)");
        let mut at_n = year("late", n);
        at_n.anchor_dues = false;
        let mut at_n1 = year("late", n + 1);
        at_n1.anchor_dues = true;
        at_n1.anchor_religious = true;
        at_n1.anchor_committee = true;
        let rows = feature_rows(
            &[household],
            &[at_n, at_n1],
            n,
            &caps(&["renewal", "school", "committee"]),
        );
        let row = &rows[0];
        assert_eq!(row.features[5], 0.0, "future dues not billed at N");
        assert_eq!(row.features[7], 0.0, "future enrolment not anchored at N");
        assert_eq!(row.features[8], 0.0, "future committee not anchored at N");
    }

    #[test]
    fn cutoff_anchors_and_history_populate_features() {
        let n = 2024;
        let mut household = hh("full", true, 2018, None, "(not coded)");
        household.tier = Some("Young Professionals".into()); // intro tier
        household.last_rs_year = Some(2020); // ended, 4 years before cutoff
        let mut at_n = year("full", n);
        at_n.anchor_dues = true;
        at_n.anchor_religious = true;
        at_n.anchor_committee = true;
        let rows = feature_rows(
            &[household],
            &[at_n],
            n,
            &caps(&["renewal", "school", "committee"]),
        );
        let f = rows[0].features;
        assert_eq!(f[0], 7.0, "tenure 2018..=2024 inclusive");
        assert_eq!(f[2], 1.0, "intro tier");
        assert_eq!(f[3], 1.0, "religious school ended");
        assert_eq!(f[4], 4.0, "years since religious school");
        assert_eq!((f[5], f[7], f[8]), (1.0, 1.0, 1.0), "cutoff-year anchors");
    }

    #[test]
    fn optional_families_are_zero_when_source_unavailable() {
        let n = 2024;
        let household = hh("nofamilies", true, 2015, None, "(not coded)");
        let mut at_n = year("nofamilies", n);
        at_n.anchor_dues = true;
        at_n.anchor_religious = true;
        at_n.anchor_committee = true;
        let rows = feature_rows(&[household], &[at_n], n, &caps(&[]));
        let row = &rows[0];
        assert_eq!(
            (row.features[5], row.features[7], row.features[8]),
            (0.0, 0.0, 0.0)
        );
        assert!(!row.has_renewal && !row.has_school && !row.has_committee);
    }

    #[test]
    fn roc_auc_rewards_ranking_and_is_half_for_no_contrast() {
        // Perfect separation: every positive scores above every negative.
        let perfect = [(0.9, 1u8), (0.8, 1), (0.4, 0), (0.1, 0)];
        assert!((roc_auc(&perfect) - 1.0).abs() < 1e-9);
        // Reversed ranking scores 0.
        let reversed = [(0.1, 1u8), (0.2, 1), (0.8, 0), (0.9, 0)];
        assert!(roc_auc(&reversed).abs() < 1e-9);
        // Tied scores average to 0.5.
        let tied = [(0.5, 1u8), (0.5, 0)];
        assert!((roc_auc(&tied) - 0.5).abs() < 1e-9);
        // No positives -> undefined contrast defaults to 0.5.
        assert!((roc_auc(&[(0.5, 0u8), (0.2, 0)]) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn lift_and_brier_measure_concentration_and_calibration() {
        // 10 rows, 2 churners, both in the top decile-ish (top 1 of 10 = 1 row).
        let mut scored: Vec<(f64, u8)> = (0..10).map(|i| (i as f64 / 10.0, 0u8)).collect();
        scored[9].1 = 1; // highest score is a churner
        scored[0].1 = 1; // base rate 0.2
        let lift = top_decile_lift(&scored);
        assert!(lift > 1.0, "the top decile concentrates churn: {lift}");
        // Brier: a confident correct prediction beats the base-rate predictor.
        let good = [(0.95, 1u8), (0.05, 0)];
        assert!(brier(&good) < baseline_brier(&good));
    }

    /// A synthetic feature row with a chosen label and a single informative feature.
    fn frow(id: &str, fy: i32, label: u8, signal: f64) -> FeatureRow {
        let mut features = [0.0; 9];
        features[0] = signal;
        FeatureRow {
            account_id: id.into(),
            fy_n: fy,
            label,
            features,
            has_renewal: false,
            has_school: false,
            has_committee: false,
            renewal_observed: false,
            school_observed: false,
            committee_observed: false,
        }
    }

    /// A row whose families are available and, when available, carry data this year.
    fn frow_fam(
        id: &str,
        fy: i32,
        label: u8,
        renewal: bool,
        school: bool,
        committee: bool,
    ) -> FeatureRow {
        let mut r = frow(id, fy, label, 0.0);
        r.has_renewal = renewal;
        r.has_school = school;
        r.has_committee = committee;
        r.renewal_observed = renewal;
        r.school_observed = school;
        r.committee_observed = committee;
        r
    }

    #[test]
    fn evaluate_removes_family_with_data_only_in_recent_years() {
        // Every source is available for the whole run, but billing statements exist only
        // from FY2022 onward: FY2018..=2021 carry no renewal data. Across the FY2018..=2022
        // training window renewal coverage is 1/5, far below MIN_COVERAGE, so the gate
        // must drop the family — availability alone must not count as coverage.
        let mut rows = dataset(true);
        for r in &mut rows {
            r.has_renewal = true;
            r.has_school = true;
            r.has_committee = true;
            r.renewal_observed = r.fy_n >= 2022;
            r.school_observed = true;
            r.committee_observed = true;
        }
        let model = evaluate(&rows, DEFAULT_ALPHA);
        assert_eq!(model.removed_families, vec!["renewal".to_string()]);
        assert!(
            !model.validation.columns.contains(&5) && !model.validation.columns.contains(&6),
            "renewal columns dropped: {:?}",
            model.validation.columns
        );
        assert!(model.validation.columns.contains(&7) && model.validation.columns.contains(&8));
        for family in ["school", "committee"] {
            let c = model.coverage.iter().find(|c| c.family == family).unwrap();
            assert!(
                c.kept && c.train == 1.0 && c.score == 1.0,
                "{family}: {c:?}"
            );
        }
    }

    #[test]
    fn unobserved_year_is_distinct_from_missing_dues_coverage() {
        // The renewal source is available. FY2024 has statement data and this active
        // household simply has no dues line: coverage missing is a real feature AND the
        // year is renewal-observed. FY2018 has no statement data at all: the mart still
        // marks the active row coverage-missing (the flags are independent), but the
        // family is unobserved, so the feature is neutralized rather than read as
        // "missing" and the gate sees the year as uncovered.
        let household = hh("h", true, 2010, None, "(not coded)");
        let mut fy24 = year("h", 2024);
        fy24.dues_coverage_missing = true;
        let mut fy18 = year("h", 2018);
        fy18.renewal_observed = false;
        fy18.dues_coverage_missing = true;
        let years = [fy24, fy18];
        let caps = caps(&["renewal"]);

        let r24 = feature_rows(std::slice::from_ref(&household), &years, 2024, &caps).remove(0);
        assert!(r24.has_renewal && r24.renewal_observed);
        assert!(family_observed(&r24, "renewal"));
        assert_eq!(r24.features[6], 1.0, "coverage missing is a real feature");

        let r18 = feature_rows(std::slice::from_ref(&household), &years, 2018, &caps).remove(0);
        assert!(r18.has_renewal, "source availability is still reported");
        assert!(!r18.renewal_observed);
        assert!(
            !family_observed(&r18, "renewal"),
            "an unobserved year is uncovered"
        );
        assert_eq!(
            (r18.features[5], r18.features[6]),
            (0.0, 0.0),
            "unobserved year is neutralized, not counted as coverage missing"
        );
    }

    #[test]
    fn full_observation_keeps_every_family_and_matches_the_ungated_model() {
        // Equivalence guard: when every year carries data for every family, coverage is
        // 1.0, nothing is removed, and the validated model is exactly the rolling
        // backtest over all nine columns — the gate contributes nothing.
        let mut rows = dataset(true);
        for r in &mut rows {
            r.has_renewal = true;
            r.has_school = true;
            r.has_committee = true;
            r.renewal_observed = true;
            r.school_observed = true;
            r.committee_observed = true;
        }
        let model = evaluate(&rows, DEFAULT_ALPHA);
        assert!(model.removed_families.is_empty());
        assert_eq!(model.coverage.len(), 3);
        assert!(model
            .coverage
            .iter()
            .all(|c| c.kept && c.train == 1.0 && c.score == 1.0));
        assert_eq!(model.validation.columns, (0..9).collect::<Vec<_>>());
        let ungated = rolling_validation(&rows, &model.validation.columns, DEFAULT_ALPHA);
        assert_eq!(model.validation.passed, ungated.passed);
        assert_eq!(model.validation.roc_auc, ungated.roc_auc);
        assert_eq!(model.validation.top_decile_lift, ungated.top_decile_lift);
        assert_eq!(model.validation.brier, ungated.brier);
        assert_eq!(model.validation.failures, ungated.failures);
    }

    #[test]
    fn evaluate_removes_family_whose_source_is_unavailable() {
        // Renewal and school available; committee absent everywhere.
        let mut rows = Vec::new();
        for fy in [2020, 2021] {
            for i in 0..4 {
                rows.push(frow_fam(
                    &format!("{fy}-{i}"),
                    fy,
                    (i % 2) as u8,
                    true,
                    true,
                    false,
                ));
            }
        }
        let model = evaluate(&rows, DEFAULT_ALPHA);
        assert!(model.removed_families.iter().any(|f| f == "committee"));
        assert!(!model.removed_families.iter().any(|f| f == "renewal"));
        // Committee column (8) is dropped; renewal (5,6) and school (7) are retained.
        assert!(!model.validation.columns.contains(&8));
        assert!(model.validation.columns.contains(&5) && model.validation.columns.contains(&7));
        assert!(model
            .coverage
            .iter()
            .any(|c| c.family == "renewal" && c.kept));
    }

    #[test]
    fn rolling_validation_fails_on_insufficient_samples() {
        // Two tiny test years, far below the 200-household / 20-exit floor.
        let mut rows = vec![frow("a", 2020, 0, 0.0), frow("b", 2020, 1, 1.0)];
        rows.push(frow("c", 2021, 0, 0.0));
        rows.push(frow("d", 2021, 1, 1.0));
        let v = rolling_validation(&rows, &[0], DEFAULT_ALPHA);
        assert!(!v.passed);
        assert!(
            v.failures.iter().any(|f| f.contains("test year")),
            "reports too few sufficient years: {:?}",
            v.failures
        );
        // FY2020 is the earliest year: no training history, so it is not a test year.
        assert!(v.years.iter().all(|y| y.test_fy != 2020));
    }

    /// Deterministic pseudo-random source (no clock/rng crate) for synthetic datasets.
    struct Lcg(u64);
    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Six fiscal years of 250 households each. With `signal`, churn tracks feature 0
    /// (with 10% label noise); without it, churn is independent of every feature.
    fn dataset(signal: bool) -> Vec<FeatureRow> {
        let mut rng = Lcg(0x9E37_79B9_7F4A_7C15);
        let mut rows = Vec::new();
        for fy in 2018..=2023 {
            for i in 0..250 {
                let x = rng.unit();
                let noise = rng.unit();
                let choose = rng.unit();
                let label = if signal {
                    let base = (x > 0.65) as u8;
                    if noise < 0.10 {
                        1 - base
                    } else {
                        base
                    }
                } else {
                    (choose < 0.35) as u8
                };
                let mut features = [0.0; 9];
                features[0] = x;
                rows.push(FeatureRow {
                    account_id: format!("{fy}-{i}"),
                    fy_n: fy,
                    label,
                    features,
                    has_renewal: false,
                    has_school: false,
                    has_committee: false,
                    renewal_observed: false,
                    school_observed: false,
                    committee_observed: false,
                });
            }
        }
        rows
    }

    #[test]
    fn a_stable_signal_passes_every_validation_gate() {
        let model = evaluate(&dataset(true), DEFAULT_ALPHA);
        assert!(
            model.validation.passed,
            "auc {:.3} lift {:.2} brier {:.4} vs {:.4}; failures {:?}",
            model.validation.roc_auc,
            model.validation.top_decile_lift,
            model.validation.brier,
            model.validation.baseline_brier,
            model.validation.failures
        );
        assert!(model.validation.roc_auc >= MIN_AUC);
        assert!(
            model
                .validation
                .years
                .iter()
                .filter(|y| y.sufficient)
                .count()
                >= MIN_TEST_YEARS
        );
    }

    #[test]
    fn no_signal_fails_validation_so_no_ranking_is_produced() {
        let model = evaluate(&dataset(false), DEFAULT_ALPHA);
        assert!(
            !model.validation.passed,
            "a signal-free model must not pass: auc {:.3} lift {:.2}",
            model.validation.roc_auc, model.validation.top_decile_lift
        );
    }

    #[test]
    fn evidence_excludes_stale_religious_school_and_counts_one_event_once() {
        let cur = 2026;
        // Religious School that ended five years ago is not current risk evidence.
        let mut stale = hh("stale", true, 2010, None, "(not coded)");
        stale.rs_family = true;
        stale.active_rs_students = 0;
        stale.last_rs_year = Some(2021); // cur - 5
        assert!(evidence_classes(&stale, &[], cur, &[]).is_empty());
        // A recent end is a single class no matter how many fields describe that one event.
        let mut recent = hh("recent", true, 2010, None, "(not coded)");
        recent.rs_family = true;
        recent.active_rs_students = 0;
        recent.last_rs_year = Some(2025); // cur - 1
        let e = evidence_classes(&recent, &[], cur, &[]);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].class, "recent_religious_school_end");
        // Two distinct events (recent RS end + intro-tier aging) make two classes.
        let mut two = hh("two", true, 2016, None, "(not coded)");
        two.rs_family = true;
        two.active_rs_students = 0;
        two.last_rs_year = Some(2025);
        two.tier = Some("Young Professionals".into());
        assert_eq!(evidence_classes(&two, &[], cur, &[]).len(), 2);
    }

    #[test]
    fn watch_list_requires_top_decile_and_two_independent_classes() {
        let rs = Evidence {
            class: "recent_religious_school_end".into(),
            detail: "x".into(),
        };
        let intro = Evidence {
            class: "intro_tier_aging".into(),
            detail: "y".into(),
        };
        let mut cands = vec![
            // Top decile, two classes -> listed.
            Candidate {
                account_id: "listed".into(),
                name: "Listed".into(),
                score: 0.98,
                evidence: vec![rs.clone(), intro.clone()],
            },
            // Top decile, but two fields from ONE event -> one class -> not listed.
            Candidate {
                account_id: "one_event".into(),
                name: "One Event".into(),
                score: 0.99,
                evidence: vec![rs.clone(), rs.clone()],
            },
        ];
        // Fillers: two classes each but low scores, so the decile gate excludes them.
        for i in 0..18 {
            cands.push(Candidate {
                account_id: format!("filler{i}"),
                name: format!("Filler {i}"),
                score: 0.1,
                evidence: vec![rs.clone(), intro.clone()],
            });
        }
        let rows = watch_list(&cands);
        assert_eq!(
            rows.len(),
            1,
            "only the top-decile two-class household lists"
        );
        assert_eq!(rows[0].account_id, "listed");
    }

    #[test]
    fn a_failing_model_suppresses_the_named_watch_list() {
        let (_model, list) = build_watch_list(&dataset(false), &[], &[], &[], 2024, DEFAULT_ALPHA);
        assert!(!list.available && list.rows.is_empty());
        assert!(list.unavailable_reason.is_some());
    }

    #[test]
    fn risk_payloads_reflect_a_suppressed_model() {
        let (model, list) = build_watch_list(&dataset(false), &[], &[], &[], 2024, DEFAULT_ALPHA);
        let summary = risk_summary(&model, &list);
        assert!(!summary.available && summary.watch_list_count == 0);
        assert!(summary.unavailable_reason.is_some());
        let view = watch_list_view(&model, &list);
        assert!(!view.available && view.rows.is_empty());
        let csv = watch_list_csv(&view);
        assert!(csv.starts_with("account_id,name,score,evidence\n"));
        assert_eq!(csv.lines().count(), 1, "header only, no names");
    }

    #[test]
    fn watch_list_csv_quotes_names_and_joins_evidence() {
        let view = WatchListView {
            available: true,
            unavailable_reason: None,
            model_first_fy: Some(2019),
            model_last_fy: Some(2023),
            baseline_rate: 0.1,
            confidence: 0.7,
            rows: vec![WatchRowView {
                account_id: "a".into(),
                name: "Fam, A".into(),
                score: 0.9,
                evidence: vec![
                    EvidenceView {
                        class: "recent_religious_school_end".into(),
                        detail: "x".into(),
                    },
                    EvidenceView {
                        class: "intro_tier_aging".into(),
                        detail: "y".into(),
                    },
                ],
            }],
        };
        let csv = watch_list_csv(&view);
        assert!(csv.contains("\"Fam, A\""), "a name with a comma is quoted");
        assert!(csv.contains("recent_religious_school_end;intro_tier_aging"));
    }
}
