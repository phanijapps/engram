//! Mechanical development-artifact sealing through Rust's accepted f32 contract.
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    io::stdin().take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("calibration exceeds 64 KiB".into());
    }
    let mut artifact: engram_domain::CodeCalibrationArtifact = serde_json::from_slice(&bytes)?;
    artifact.id = engram_integration::code_evidence::artifact_id(&artifact)?;
    println!("{}", serde_json::to_string(&artifact)?);
    Ok(())
}
