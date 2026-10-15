//! Model indices are authoritative; sorted model output is never input order.
use engram_runtime::{CoreError, CoreResult};
pub(super) fn remap(
    count: usize,
    results: impl IntoIterator<Item = (usize, f32)>,
) -> CoreResult<Vec<f32>> {
    let mut output = vec![None; count];
    for (index, score) in results {
        if index >= count || !score.is_finite() || output[index].replace(score).is_some() {
            return Err(invalid());
        }
    }
    output
        .into_iter()
        .map(|score| score.ok_or_else(invalid))
        .collect()
}
fn invalid() -> CoreError {
    CoreError::ValidationFailed {
        reason: "local scorer output count/index/score is invalid".into(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_is_remapped_and_partial_duplicate_invalid_results_are_rejected() {
        assert_eq!(
            remap(3, [(2, 0.2), (0, 0.9), (1, -0.1)]).unwrap(),
            [0.9, -0.1, 0.2]
        );
        assert!(remap(3, [(0, 1.0)]).is_err());
        assert!(remap(2, [(0, 1.0), (0, 0.1)]).is_err());
        assert!(remap(1, [(1, 1.0)]).is_err());
        assert!(remap(1, [(0, f32::NAN)]).is_err());
    }
}
