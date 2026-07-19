# Source-Type Extensibility Pattern

This document verifies that adding a new source type to the ingestion adapter follows the Open/Closed Principle (OCP) — it requires only a trait implementation, not edits to shared orchestration code.

## Pattern: Add a New Source Type

To add extraction for a new source type (e.g., Excel spreadsheets, DB rows):

1. **Create a new extractor module** under `adapters/ingest/src/extractors/`:
   ```rust
   // adapters/ingest/src/extractors/excel.rs
   use engram_domain::*;
   use engram_knowledge::CoreResult;
   use crate::chunker::Chunker;
   use crate::extractors::SourceExtractor;
   
   pub struct ExcelExtractor;
   
   impl SourceExtractor for ExcelExtractor {
       fn extract(
           &self,
           document: &SourceDocument,
           chunks: &[KnowledgeChunk],
           scope: &Scope,
       ) -> CoreResult<(Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>)> {
           // Excel-specific extraction logic
           Ok((entities, relationships))
       }
       
       fn select_chunker(&self) -> CoreResult<Box<dyn Chunker>> {
           // Return Excel-appropriate chunker
       }
   }
   ```

2. **Add the module to `extractors/mod.rs`**:
   ```rust
   mod excel;
   pub use excel::ExcelExtractor;
   ```

3. **Register in dispatch** (when ready to integrate): Add to source-type selection logic.

## What You DON'T Need to Edit

- ❌ No edits to `scanner.rs` match statements (dispatch is trait-based)
- ❌ No edits to `extractor.rs` kind branches (logic is in trait impls)
- ❌ No edits to shared chunking or persistence logic
- ❌ No changes to `GraphExtractor` or existing extractors

## Verification

The existing `StructuredExtractor` stub demonstrates this pattern:
- ✅ Implements `SourceExtractor` trait (line 27-48)
- ✅ Added to `extractors/mod.rs` exports
- ✅ Returns `Ok(vec![])` as placeholder behavior
- ✅ Can be expanded to full Excel/DB implementation without breaking existing code

## OCP Compliance

- **Open for extension**: New source types are additive trait implementations
- **Closed for modification**: Shared orchestration code (scanner, ingestor, persistence) remains unchanged
- **Behavior preservation**: Existing code/docs/contract extraction unaffected by new types

## Example: Adding PDF Extraction

Future PDF extraction would follow the same pattern:

```rust
// adapters/ingest/src/extractors/pdf.rs
pub struct PdfExtractor;

impl SourceExtractor for PdfExtractor {
    fn extract(&self, document: &SourceDocument, chunks: &[KnowledgeChunk], scope: &Scope) 
        -> CoreResult<(Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>) 
    {
        // PDF parsing logic
    }
    
    fn select_chunker(&self) -> CoreResult<Box<dyn Chunker>> {
        // PDF-aware chunker
    }
}
```

No scanner edits, no extractor edits, no test changes to existing functionality.
