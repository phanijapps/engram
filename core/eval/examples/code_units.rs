//! Emit parser-owned canonical ranges for blind source-label verification.
//! This example owns file I/O; the parser remains pure.
use engram_code::TreeSitterChunker;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = TreeSitterChunker::new()?;
    let mut units = Vec::new();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path)?;
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .ok_or("extension missing")?;
        let tree = parser.parse(&text, ext)?;
        for chunk in parser.covered_chunks_with_tree(&tree, &text, ext)? {
            if let Some(location) = &chunk.candidate.location {
                units.push(
                    json!({"path":path,"anchor":location.anchor,"ranges":chunk.ranges,
                    "role":chunk.role,"complete":chunk.complete}),
                );
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&units)?);
    Ok(())
}
