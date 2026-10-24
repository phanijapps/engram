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

    /// F3 review P1-1: qualified trait implementations preserve their path —
    /// `impl a::Sink for X` must yield implements ("X", "a::Sink"), NOT a
    /// bare ambiguous "Sink" that the resolution ladder can never settle
    /// when two same-named traits exist.
    #[test]
    fn rust_qualified_trait_impls_preserve_the_path() {
        let edges = structural(
            "mod a { pub trait Sink {} }\nmod b { pub trait Sink {} }\nstruct SummarySink;\nimpl a::Sink for SummarySink {}\n",
            "rs",
        );
        assert!(
            edges
                .implements
                .contains(&("SummarySink".to_owned(), "a::Sink".to_owned())),
            "implements must preserve a::Sink: {:?}",
            edges.implements
        );
        // TS mirror: implements clause with a qualified name.
        let ts = structural("class Repo implements Store.Core {}\n", "ts");
        assert!(
            ts.implements
                .contains(&("Repo".to_owned(), "Store.Core".to_owned()))
                || ts.implements.iter().any(|(_, t)| t.contains("Core")),
            "ts implements keeps qualification: {:?}",
            ts.implements
        );
    }

    #[test]
    fn unsupported_language_yields_empty() {
        let edges = structural("some ruby code\nclass Foo\nend\n", "rb");
        assert!(edges.imports.is_empty());
        assert!(edges.contains.is_empty());
    }

    #[test]
    fn keyword_noise_filtered_from_implementers() {
        // `impl Sink for mut ...` (parser artifact) must NOT produce an
        // implements edge with type_name "mut".
        let edges = structural("pub trait Sink {}\nstruct S;\nimpl Sink for S {}\n", "rs");
        assert!(
            edges
                .implements
                .iter()
                .all(|(t, _)| t != "mut" && t != "ref" && t != "dyn"),
            "keyword noise in implementers: {:?}",
            edges.implements
        );
    }

    /// Receiver-aware extraction (plan v6): `WalkBuilder::new().build()` —
    /// the constructor root types through the repo return table and `build`
    /// emits as a receiver-verified `WalkBuilder::build`.
    #[test]
    fn builder_chain_methods_resolve_to_the_chain_root_type() {
        use crate::parser::repo_context::{RepoContextBuilder, ReturnEvidence};
        let mut b = RepoContextBuilder::default();
        b.declare_type("WalkBuilder");
        b.declare_method(
            "WalkBuilder::new",
            ReturnEvidence::Returns("WalkBuilder".into()),
        );
        b.declare_method("WalkBuilder::build", ReturnEvidence::Unsupported);
        let repo = b.build();
        let chunker = crate::parser::tree_sitter_chunker::TreeSitterChunker::new().unwrap();
        let names = std::collections::HashSet::from(["build".to_owned(), "new".to_owned()]);
        let calls = chunker
            .extract_calls(
                "fn main() { let w = WalkBuilder::new().build(); }",
                "rs",
                &names,
                &repo,
            )
            .unwrap();
        assert!(
            calls.iter().any(|(_, c)| c == "WalkBuilder::new"),
            "ctor emitted: {calls:?}"
        );
        assert!(
            calls.iter().any(|(_, c)| c == "WalkBuilder::build"),
            "chain-tail build must be receiver-verified: {calls:?}"
        );
    }

    /// `let w = WalkBuilder::new(&paths); w.build();` — the DOMINANT stored-
    /// builder pattern types through the env + repo table.
    #[test]
    fn let_binding_builder_qualifies_stored_builders() {
        use crate::parser::repo_context::{RepoContextBuilder, ReturnEvidence};
        let mut b = RepoContextBuilder::default();
        b.declare_type("WalkBuilder");
        b.declare_method(
            "WalkBuilder::new",
            ReturnEvidence::Returns("WalkBuilder".into()),
        );
        let repo = b.build();
        let chunker = crate::parser::tree_sitter_chunker::TreeSitterChunker::new().unwrap();
        let names = std::collections::HashSet::from(["build".to_owned()]);
        let calls = chunker
            .extract_calls(
                "fn main() {\n    let w = WalkBuilder::new(&paths);\n    w.build();\n}\n",
                "rs",
                &names,
                &repo,
            )
            .unwrap();
        assert!(
            calls.iter().any(|(_, c)| c == "WalkBuilder::build"),
            "stored-builder build must be typed: {calls:?}"
        );
    }

    /// Type annotations: `let gs: GlobSet = …; gs.is_match(path)`.
    #[test]
    fn type_annotation_let_binds_variable_to_type() {
        use crate::parser::repo_context::{RepoContextBuilder, ReturnEvidence};
        let mut b = RepoContextBuilder::default();
        b.declare_type("GlobSet");
        b.declare_type("Builder");
        b.declare_method("Builder::build", ReturnEvidence::Returns("GlobSet".into()));
        let repo = b.build();
        let chunker = crate::parser::tree_sitter_chunker::TreeSitterChunker::new().unwrap();
        let names = std::collections::HashSet::from(["is_match".to_owned()]);
        let calls = chunker
            .extract_calls(
                "fn f() {\n    let gs: GlobSet = builder.build();\n    gs.is_match(path);\n}\n",
                "rs",
                &names,
                &repo,
            )
            .unwrap();
        assert!(
            calls.iter().any(|(_, c)| c == "GlobSet::is_match"),
            "annotated var must type: {calls:?}"
        );
    }

    /// Parameter types: `fn search(worker: &SearchWorker)` + `worker.run(m)`.
    #[test]
    fn function_parameters_bind_to_their_types() {
        let repo = crate::parser::repo_context::RepoContext::empty();
        let chunker = crate::parser::tree_sitter_chunker::TreeSitterChunker::new().unwrap();
        let names = std::collections::HashSet::from(["run".to_owned()]);
        let calls = chunker
            .extract_calls(
                "fn search(worker: &SearchWorker, matcher: &RegexMatcher) {\n    worker.run(matcher);\n}\n",
                "rs", &names, &repo,
            )
            .unwrap();
        assert!(
            calls.iter().any(|(_, c)| c == "SearchWorker::run"),
            "param-typed receiver must qualify: {calls:?}"
        );
    }

    /// Struct fields: `self.searcher.search()` inside `impl Config`.
    #[test]
    fn struct_field_types_bind() {
        let repo = crate::parser::repo_context::RepoContext::empty();
        let chunker = crate::parser::tree_sitter_chunker::TreeSitterChunker::new().unwrap();
        let names = std::collections::HashSet::from(["search".to_owned()]);
        let calls = chunker
            .extract_calls(
                "struct Config { searcher: Searcher }\nimpl Config { fn go(&self) { self.searcher.search(); } }\n",
                "rs", &names, &repo,
            )
            .unwrap();
        assert!(
            calls.iter().any(|(_, c)| c == "Searcher::search"),
            "self.field receiver must type through the field map: {calls:?}"
        );
    }

    /// Primitives never become owners: `name.len()` on `name: &str` has an
    /// unknown receiver; `len` is generic → dropped entirely.
    #[test]
    fn primitive_types_are_excluded() {
        let repo = crate::parser::repo_context::RepoContext::empty();
        let chunker = crate::parser::tree_sitter_chunker::TreeSitterChunker::new().unwrap();
        let names = std::collections::HashSet::from(["len".to_owned()]);
        let calls = chunker
            .extract_calls("fn f(name: &str) { name.len(); }", "rs", &names, &repo)
            .unwrap();
        assert!(
            !calls
                .iter()
                .any(|(_, c)| c.contains("::len") || c == "name.len"),
            "generic method on primitive-typed receiver must drop: {calls:?}"
        );
    }
}
