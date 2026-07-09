//! Temporal scoring modes over versioned symbols.

use chrono::{DateTime, Utc};

/// A versioned symbol to score: its bi-temporal validity interval (ADR-0019) and
/// its call-graph in/out degree (caller-supplied — computed from `calls` edges
/// via `engram-graph-analytics`).
#[derive(Debug, Clone, PartialEq)]
pub struct VersionedSymbol {
    pub key: String,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub in_degree: usize,
    pub out_degree: usize,
}

impl VersionedSymbol {
    /// True if this version is currently valid: `valid_from` at or before `now`,
    /// and no `valid_until` at or before `now`.
    fn is_current_at(&self, now: DateTime<Utc>) -> bool {
        let Some(from) = self.valid_from else {
            return false;
        };
        if from > now {
            return false;
        }
        !matches!(self.valid_until, Some(until) if until <= now)
    }
}

/// Recency score: `2^(-elapsed/half_life)` seconds for currently-valid versions,
/// else 0.0. A version introduced exactly `now` scores 1.0; `half_life <= 0`
/// treats any positive elapsed time as fully decayed (0.0).
fn recency(symbol: &VersionedSymbol, now: DateTime<Utc>, half_life: f64) -> f64 {
    let Some(from) = symbol.valid_from else {
        return 0.0;
    };
    if !symbol.is_current_at(now) {
        return 0.0;
    }
    let elapsed = (now - from).num_seconds().max(0) as f64;
    if elapsed == 0.0 {
        return 1.0;
    }
    if half_life <= 0.0 {
        return 0.0;
    }
    2f64.powf(-elapsed / half_life)
}

/// `recent` mode: ranks versioned symbols by recency, best-first.
pub fn recent(
    versions: &[VersionedSymbol],
    now: DateTime<Utc>,
    half_life: f64,
) -> Vec<(String, f64)> {
    let mut ranked: Vec<(String, f64)> = versions
        .iter()
        .map(|v| (v.key.clone(), recency(v, now, half_life)))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

/// Blast-radius-weighted impact: `in_degree^0.7 × (1 + out_degree)^0.3`.
fn impact_of(symbol: &VersionedSymbol) -> f64 {
    (symbol.in_degree as f64).powf(0.7) * ((1 + symbol.out_degree) as f64).powf(0.3)
}

/// `impact` mode: ranks by blast-radius-weighted impact, best-first.
pub fn impact(versions: &[VersionedSymbol]) -> Vec<(String, f64)> {
    let mut ranked: Vec<(String, f64)> = versions
        .iter()
        .map(|v| (v.key.clone(), impact_of(v)))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

/// `compound` mode: a max-normalized blend (0.5 recency + 0.5 impact) of the two
/// legs, best-first. Normalization lets the very different scales compose.
pub fn compound(
    versions: &[VersionedSymbol],
    now: DateTime<Utc>,
    half_life: f64,
) -> Vec<(String, f64)> {
    let recency_scores: Vec<f64> = versions
        .iter()
        .map(|v| recency(v, now, half_life))
        .collect();
    let impact_scores: Vec<f64> = versions.iter().map(impact_of).collect();
    let max_recency = recency_scores.iter().cloned().fold(0.0_f64, f64::max);
    let max_impact = impact_scores.iter().cloned().fold(0.0_f64, f64::max);

    let mut ranked: Vec<(String, f64)> = versions
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let norm_recency = if max_recency > 0.0 {
                recency_scores[i] / max_recency
            } else {
                0.0
            };
            let norm_impact = if max_impact > 0.0 {
                impact_scores[i] / max_impact
            } else {
                0.0
            };
            (v.key.clone(), 0.5 * norm_recency + 0.5 * norm_impact)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).single().unwrap()
    }

    /// A timestamp `offset_seconds` from the fixed `now` (negative = past).
    fn at(offset_seconds: i64) -> DateTime<Utc> {
        now() + chrono::Duration::seconds(offset_seconds)
    }

    #[allow(clippy::too_many_arguments)]
    fn sym(
        key: &str,
        from_offset: Option<i64>,
        in_degree: usize,
        out_degree: usize,
    ) -> VersionedSymbol {
        VersionedSymbol {
            key: key.to_owned(),
            valid_from: from_offset.map(at),
            valid_until: None,
            in_degree,
            out_degree,
        }
    }

    #[test]
    fn recent_ranks_introduced_now_above_old() {
        let versions = vec![
            sym("old", Some(-10 * 3600), 0, 0), // 10 half-lives ago -> ~0
            sym("fresh", Some(0), 0, 0),        // now -> 1
        ];
        let ranked = recent(&versions, now(), 3600.0);
        assert_eq!(ranked[0].0, "fresh");
        assert!(ranked[0].1 > 0.99);
        assert_eq!(ranked[1].0, "old");
        assert!(ranked[1].1 < 0.01);
    }

    #[test]
    fn impact_ranks_high_in_degree_first() {
        let versions = vec![
            sym("sink", None, 10, 0),   // high in-degree
            sym("source", None, 0, 10), // high out-degree, zero in
        ];
        let ranked = impact(&versions);
        assert_eq!(ranked[0].0, "sink");
        assert!(ranked[0].1 > 0.0);
        assert_eq!(ranked[1].0, "source");
        assert_eq!(ranked[1].1, 0.0); // 0^0.7 = 0
    }

    #[test]
    fn compound_blends_recency_and_impact() {
        // fresh AND high-impact vs old AND low-impact.
        let versions = vec![
            sym("star", Some(0), 10, 2),
            sym("ghost", Some(-10 * 3600), 0, 0),
        ];
        let ranked = compound(&versions, now(), 3600.0);
        assert_eq!(ranked[0].0, "star");
        assert!(ranked[0].1 > ranked[1].1);
    }

    #[test]
    fn versions_without_valid_from_score_zero_under_recent() {
        let versions = vec![
            sym("no_version", None, 5, 5),
            sym("versioned", Some(-60), 5, 5),
        ];
        let ranked = recent(&versions, now(), 3600.0);
        let no_version = ranked.iter().find(|(k, _)| k == "no_version").unwrap();
        assert_eq!(no_version.1, 0.0);
        assert!(!no_version.1.is_nan());
    }

    #[test]
    fn expired_versions_score_zero_under_recent() {
        // valid_until in the past -> not current -> 0.
        let mut expired = sym("expired", Some(-3600), 5, 5);
        expired.valid_until = Some(at(-60));
        let versions = vec![expired, sym("current", Some(-60), 5, 5)];
        let ranked = recent(&versions, now(), 3600.0);
        let expired_score = ranked.iter().find(|(k, _)| k == "expired").unwrap().1;
        assert_eq!(expired_score, 0.0);
    }
}
