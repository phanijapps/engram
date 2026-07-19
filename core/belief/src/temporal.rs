//! Valid-time predicates for belief compatibility.
//!
//! Belief records use valid-time intervals to describe when a stance applies.
//! These helpers intentionally do not implement record-time history; repository
//! implementations must reject record-time queries unless their storage keeps
//! historical versions.

use engram_domain::Timestamp;

/// Returns true when `as_of` falls within a start-inclusive, end-exclusive
/// valid-time interval.
///
/// Missing starts and ends are unbounded. End-exclusive matching keeps adjacent
/// supersession intervals from both matching at the boundary.
pub fn interval_contains(
    valid_from: Option<Timestamp>,
    valid_until: Option<Timestamp>,
    as_of: Timestamp,
) -> bool {
    valid_from.is_none_or(|start| start <= as_of) && valid_until.is_none_or(|end| as_of < end)
}

/// Returns true when a belief interval is live at `as_of`.
///
/// This is a naming wrapper for callers that are filtering complete belief
/// records and want the valid-time rule to read at the call site.
pub fn live_at(
    valid_from: Option<Timestamp>,
    valid_until: Option<Timestamp>,
    as_of: Timestamp,
) -> bool {
    interval_contains(valid_from, valid_until, as_of)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::*;

    fn ts(seconds: i64) -> Timestamp {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    #[test]
    fn interval_is_start_inclusive_and_end_exclusive() {
        let start = ts(10);
        let end = ts(20);

        assert!(interval_contains(Some(start), Some(end), start));
        assert!(interval_contains(
            Some(start),
            Some(end),
            start + Duration::seconds(1)
        ));
        assert!(!interval_contains(Some(start), Some(end), end));
    }

    #[test]
    fn open_interval_bounds_are_unbounded() {
        assert!(interval_contains(None, None, ts(0)));
        assert!(interval_contains(None, Some(ts(10)), ts(-100)));
        assert!(interval_contains(Some(ts(10)), None, ts(100)));
        assert!(!interval_contains(Some(ts(10)), None, ts(9)));
        assert!(!interval_contains(None, Some(ts(10)), ts(10)));
    }
}
