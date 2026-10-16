//! Offline evaluation only; expected-answer judgments never enter retrieval.
use engram_eval::code_intelligence::{CodeJudgment, ObservedCodeUnit, evaluate_code_query};
use serde::Deserialize;
use std::io::{self, BufRead, Write};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    judgments: Vec<CodeJudgment>,
    candidate_parents: Vec<String>,
    primary: Vec<ObservedCodeUnit>,
    supporting: Vec<ObservedCodeUnit>,
    validated: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let metrics = evaluate_code_query(
            &input.judgments,
            &input.candidate_parents,
            &input.primary,
            &input.supporting,
            input.validated,
        )?;
        writeln!(out, "{}", serde_json::to_string(&metrics)?)?;
        out.flush()?;
    }
    Ok(())
}
