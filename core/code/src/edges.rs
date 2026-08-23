//! Typed structural edges (RFC-0020 Phase 2): imports, contains, extends,
//! implements — extracted from the AST alongside `calls`.
//!
//! Pure output: name pairs (receiver-qualified where the endpoints are
//! declarations); the extractor attaches entity refs, ids, and provenance.
//! Language coverage for imports/inheritance starts with Rust, Python,
//! TS/JS, and Go; languages without a mapped node kind emit nothing (never a
//! parse failure).

/// Structural facts for one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructuralEdges {
    /// Import path strings as written (`std::collections`, `./utils`,
    /// `os.path`). One entry per import statement; the extractor forms the
    /// file→module edge.
    pub imports: Vec<String>,
    /// (parent, member) containment pairs from the receiver chain:
    /// `impl Foo { fn bar }` yields (`Foo`, `Foo::bar`).
    pub contains: Vec<(String, String)>,
    /// (child, base) inheritance: `class A extends B`, Python bases.
    pub extends: Vec<(String, String)>,
    /// (child, interface) conformance: TS `implements`, Rust
    /// `impl Trait for Type` (Type implements Trait).
    pub implements: Vec<(String, String)>,
}

impl StructuralEdges {
    pub fn is_empty(&self) -> bool {
        self.imports.is_empty()
            && self.contains.is_empty()
            && self.extends.is_empty()
            && self.implements.is_empty()
    }
}

/// Containment pairs for a declaration emitted with the given receiver
/// chain: every ancestor prefix pairs with the next level (`A` contains
/// `A::B` contains `A::B::c`).
pub fn contains_pairs(receivers: &[String], name: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if receivers.is_empty() {
        return out;
    }
    let mut prefix = receivers[0].clone();
    for next in &receivers[1..] {
        let child = format!("{prefix}::{next}");
        out.push((prefix.clone(), child.clone()));
        prefix = child;
    }
    // The member edge uses the member's receiver-qualified name.
    out.push((prefix, format!("{}::{}", receivers.join("::"), name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_pairs_chain_every_level() {
        let receivers = ["Foo".to_owned(), "inner".to_owned()];
        assert_eq!(
            contains_pairs(&receivers, "run"),
            vec![
                ("Foo".to_owned(), "Foo::inner".to_owned()),
                ("Foo::inner".to_owned(), "Foo::inner::run".to_owned()),
            ]
        );
    }

    #[test]
    fn contains_pairs_single_receiver() {
        let receivers = ["Foo".to_owned()];
        assert_eq!(
            contains_pairs(&receivers, "bar"),
            vec![("Foo".to_owned(), "Foo::bar".to_owned())]
        );
    }

    #[test]
    fn contains_pairs_top_level_is_empty() {
        assert!(contains_pairs(&[], "parse").is_empty());
    }

    #[test]
    fn structural_edges_empty_default() {
        assert!(StructuralEdges::default().is_empty());
    }
}

#[cfg(test)]
mod extraction_tests {
    use super::StructuralEdges;
    use crate::parser::tree_sitter_chunker::TreeSitterChunker;

    fn structural(source: &str, ext: &str) -> StructuralEdges {
        TreeSitterChunker::new()
            .expect("chunker")
            .extract_structural(source, ext)
            .expect("structural")
    }

    #[test]
    fn rust_imports_and_impl_conformance() {
        let edges = structural(
            "use std::collections::HashMap;\nuse crate::util::{a, b};\ntrait Store {}\nimpl Store for Cache {}\n",
            "rs",
        );
        assert!(
            edges
                .imports
                .contains(&"std::collections::HashMap".to_owned())
        );
        assert!(edges.imports.contains(&"crate::util".to_owned()));
        assert!(
            edges
                .implements
                .contains(&("Cache".to_owned(), "Store".to_owned())),
            "implements: {:?}",
            edges.implements
        );
    }

    #[test]
    fn rust_containment_from_impl_methods() {
        let edges = structural("struct Foo;\nimpl Foo {\n    fn bar(&self) {}\n}\n", "rs");
        assert!(
            edges
                .contains
                .contains(&("Foo".to_owned(), "Foo::bar".to_owned())),
            "contains: {:?}",
            edges.contains
        );
    }

    #[test]
    fn python_imports_and_bases() {
        let edges = structural(
            "import os.path\nfrom .util import helper\nclass Dog(Animal):\n    def speak(self):\n        pass\n",
            "py",
        );
        assert!(edges.imports.contains(&"os.path".to_owned()));
        assert!(edges.imports.contains(&".util".to_owned()));
        assert!(
            edges
                .extends
                .contains(&("Dog".to_owned(), "Animal".to_owned())),
            "extends: {:?}",
            edges.extends
        );
        assert!(
            edges
                .contains
                .contains(&("Dog".to_owned(), "Dog::speak".to_owned())),
            "contains: {:?}",
            edges.contains
        );
    }

    #[test]
    fn typescript_heritage_and_module_imports() {
        let edges = structural(
            "import { helper } from './utils';\nclass Repo extends Base implements Store {\n    find(): void {}\n}\n",
            "ts",
        );
        assert!(edges.imports.contains(&"./utils".to_owned()));
        assert!(
            edges
                .extends
                .contains(&("Repo".to_owned(), "Base".to_owned())),
            "extends: {:?}",
            edges.extends
        );
        assert!(
            edges
                .implements
                .contains(&("Repo".to_owned(), "Store".to_owned())),
            "implements: {:?}",
            edges.implements
        );
    }

    #[test]
    fn python_comma_imports_fan_out() {
        let edges = structural("import os, sys as system\n", "py");
        assert!(
            edges.imports.contains(&"os".to_owned()),
            "{:?}",
            edges.imports
        );
        assert!(
            edges.imports.contains(&"sys".to_owned()),
            "{:?}",
            edges.imports
        );
        assert!(
            !edges.imports.iter().any(|i| i.contains(" as ")),
            "{:?}",
            edges.imports
        );
    }

    #[test]
    fn go_import_paths() {
        let edges = structural("package main\nimport \"fmt\"\nfunc main() {}\n", "go");
        assert!(edges.imports.contains(&"fmt".to_owned()));
    }

    #[test]
    fn unsupported_language_yields_empty() {
        let edges = structural("some ruby code\nclass Foo\nend\n", "rb");
        assert!(edges.imports.is_empty());
        assert!(edges.contains.is_empty());
    }
}
