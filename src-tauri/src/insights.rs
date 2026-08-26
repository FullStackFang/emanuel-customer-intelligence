//! Membership insights: fiscal-year math, the household mart, and the views
//! the Insights page renders. Reads the mirror only; never Salesforce.

/// First month of the fiscal year (June). FY is labeled by the calendar year it ends in.
pub const FY_START_MONTH: u32 = 6;
/// Dates outside this window are placeholders (2199-…, 2991-…) or garbage.
const MIN_YEAR: i32 = 1900;
const MAX_YEAR: i32 = 2035;

pub fn fy_from_ymd(y: i32, m: u32) -> i32 {
    if m >= FY_START_MONTH {
        y + 1
    } else {
        y
    }
}

/// Fiscal year of a Salesforce date/datetime string (`YYYY-MM-DD…`), or None if
/// unparsable or outside the plausible window.
pub fn fy_of(date: &str) -> Option<i32> {
    let d = date.get(0..10)?;
    let nd = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
    let y = chrono::Datelike::year(&nd);
    if !(MIN_YEAR..=MAX_YEAR).contains(&y) {
        return None;
    }
    Some(fy_from_ymd(y, chrono::Datelike::month(&nd)))
}

pub fn current_fy() -> i32 {
    let today = chrono::Utc::now().date_naive();
    fy_from_ymd(
        chrono::Datelike::year(&today),
        chrono::Datelike::month(&today),
    )
}

/// (mart column key, phrase to look for in `Join_Reason__c`). Order is stable: it is
/// the index into `channel_flags` and the mart's `ch_*` columns.
pub const CHANNELS: [(&str, &str); 12] = [
    ("religious_school", "religious school"),
    ("nursery_school", "nursery school"),
    ("affiliation", "affiliation"),
    ("life_cycle", "life cycle event"),
    ("family", "to be with family"),
    ("young_professionals", "young professionals"),
    ("community", "community"),
    ("hhd_tickets", "high holy day"),
    ("streicker", "streicker"),
    ("clergy", "clergy"),
    ("worship", "worship"),
    ("move", "move"),
];

pub fn channel_flags(join_reason: Option<&str>) -> [bool; 12] {
    let mut out = [false; 12];
    if let Some(jr) = join_reason {
        let l = jr.to_lowercase();
        for (i, (_, phrase)) in CHANNELS.iter().enumerate() {
            out[i] = l.contains(phrase);
        }
    }
    out
}

/// Coded resign reason -> display group. First match wins, in this order.
pub fn reason_group(raw: Option<&str>) -> &'static str {
    let Some(r) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return "(not coded)";
    };
    let l = r.to_lowercase();
    const RULES: [(&str, &str); 9] = [
        ("moved", "Moved"),
        ("non-payment", "Non-payment"),
        ("no longer engaged", "No longer engaged"),
        ("deceased", "Deceased"),
        ("aged out", "Young-adult tier aged out"),
        ("another synagogue", "Joined another synagogue"),
        ("elderly", "Elderly / ill"),
        ("financial", "Financial hardship"),
        ("displeased", "Displeased"),
    ];
    for (needle, group) in RULES {
        if l.contains(needle) {
            return group;
        }
    }
    "Other"
}

/// `LastYearAttendedRS__c` is "2025-2026" or "2007"; take the last 4-digit year.
pub fn parse_rs_year(s: Option<&str>) -> Option<i32> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    s.rsplit('-').next()?.trim().parse::<i32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiscal_year_starts_june_first_and_is_labeled_by_end_year() {
        assert_eq!(fy_from_ymd(2024, 5), 2024); // May 2024 -> FY2024 (Jun 2023 – May 2024)
        assert_eq!(fy_from_ymd(2024, 6), 2025); // Jun 2024 -> FY2025
        assert_eq!(fy_of("2024-05-31"), Some(2024));
        assert_eq!(fy_of("2024-06-01"), Some(2025));
        assert_eq!(fy_of("2001-03-28T00:00:00Z"), Some(2001));
    }

    #[test]
    fn fy_of_rejects_placeholder_and_garbage_dates() {
        assert_eq!(fy_of("2199-06-02"), None);
        assert_eq!(fy_of("2991-01-01"), None);
        assert_eq!(fy_of("1899-12-31"), None);
        assert_eq!(fy_of(""), None);
        assert_eq!(fy_of("not a date"), None);
        assert_eq!(fy_of("2024-13-01"), None);
    }

    #[test]
    fn channel_flags_split_the_multipicklist_case_insensitively() {
        let f = channel_flags(Some(
            "Nursery School and Religious School;To be with Family",
        ));
        let idx = |k: &str| CHANNELS.iter().position(|(key, _)| *key == k).unwrap();
        assert!(f[idx("religious_school")]);
        assert!(f[idx("nursery_school")]);
        assert!(f[idx("family")]);
        assert!(!f[idx("clergy")]);
        assert_eq!(channel_flags(None), [false; 12]);
        assert!(channel_flags(Some("high holy day tickets"))[idx("hhd_tickets")]);
    }

    #[test]
    fn reason_group_buckets_in_priority_order() {
        assert_eq!(reason_group(Some("Moved;No Longer Engaged")), "Moved");
        assert_eq!(reason_group(Some("Non-payment")), "Non-payment");
        assert_eq!(
            reason_group(Some("CJM / AM Aged Out")),
            "Young-adult tier aged out"
        );
        assert_eq!(
            reason_group(Some("Joined Another Synagogue")),
            "Joined another synagogue"
        );
        assert_eq!(reason_group(Some("Elderly / Ill")), "Elderly / ill");
        assert_eq!(reason_group(Some("Something new")), "Other");
        assert_eq!(reason_group(Some("")), "(not coded)");
        assert_eq!(reason_group(None), "(not coded)");
    }

    #[test]
    fn parse_rs_year_takes_the_end_year_of_a_school_year() {
        assert_eq!(parse_rs_year(Some("2025-2026")), Some(2026));
        assert_eq!(parse_rs_year(Some("2007")), Some(2007));
        assert_eq!(parse_rs_year(Some("")), None);
        assert_eq!(parse_rs_year(None), None);
    }
}
