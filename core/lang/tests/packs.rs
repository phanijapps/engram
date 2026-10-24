//! Per-language fixtures (review round-1 acceptance): a pack ships only
//! when these prove real symbol anchors + conservative call handling.
//! Exact anchors/pairs are asserted — metadata-only or `"file"` fallback
//! chunks never satisfy these.

use engram_code::TreeSitterChunker;

fn chunker() -> TreeSitterChunker {
    let mut c = TreeSitterChunker::new().expect("chunker");
    engram_lang::register_all(&mut c);
    c
}

fn anchors(c: &TreeSitterChunker, src: &str, ext: &str) -> Vec<String> {
    c.chunk_with_ext(src, ext)
        .expect("chunks")
        .iter()
        .filter_map(|ch| ch.location.as_ref().and_then(|l| l.anchor.clone()))
        .collect()
}

fn calls(c: &TreeSitterChunker, src: &str, ext: &str, names: &[&str]) -> Vec<(String, String)> {
    let set: std::collections::HashSet<String> = names.iter().map(|s| s.to_string()).collect();
    c.extract_calls(
        src,
        ext,
        &set,
        &engram_code::parser::repo_context::RepoContext::empty(),
    )
    .expect("calls")
}

// ── Lua ────────────────────────────────────────────────────────────────
#[test]
fn lua_symbols_and_calls() {
    let c = chunker();
    let src = "local function helper(x) return x end\nfunction M.run(a) helper(a) end\n";
    let a = anchors(&c, src, "lua");
    assert!(a.contains(&"fn helper".to_owned()), "anchors: {a:?}");
    assert!(a.contains(&"fn run".to_owned()), "anchors: {a:?}");
    let cl = calls(&c, src, "lua", &["helper", "run"]);
    assert!(
        cl.contains(&("run".to_owned(), "helper".to_owned())),
        "calls: {cl:?}"
    );
}

/// Review F3 negative: a generic method on an unknown Lua receiver must be
/// DROPPED (never bare-bindable cross-type), and a non-generic method keeps
/// its honest var hint.
#[test]
fn lua_receiver_discipline() {
    let c = chunker();
    let src = "function runner(values)\n  values.iter()\n  values.prepare()\nend\n";
    let cl = calls(&c, src, "lua", &["iter", "prepare", "Values"]);
    assert!(
        !cl.iter().any(|(_, callee)| callee.contains("iter")),
        "generic-on-unknown must drop entirely: {cl:?}"
    );
    assert!(
        cl.iter().any(|(_, callee)| callee == "values.prepare"),
        "non-generic keeps the honest var hint: {cl:?}"
    );
}

// ── Julia ──────────────────────────────────────────────────────────────
#[test]
fn julia_symbols() {
    let c = chunker();
    let src =
        "function helper(x)\n    return x\nend\nstruct Point\n    x::Int\nend\nmodule Utils\nend\n";
    let a = anchors(&c, src, "jl");
    for expected in ["fn helper", "struct Point", "module Utils"] {
        assert!(
            a.contains(&expected.to_owned()),
            "missing {expected}: {a:?}"
        );
    }
}

// ── Zig ────────────────────────────────────────────────────────────────
#[test]
fn zig_symbols_and_calls() {
    let c = chunker();
    let src = "pub fn helper(x: u32) u32 {\n    return x;\n}\npub fn run(a: u32) u32 {\n    return helper(a);\n}\n";
    let a = anchors(&c, src, "zig");
    assert!(a.contains(&"fn helper".to_owned()), "anchors: {a:?}");
    assert!(a.contains(&"fn run".to_owned()), "anchors: {a:?}");
    let cl = calls(&c, src, "zig", &["helper"]);
    assert!(
        cl.contains(&("run".to_owned(), "helper".to_owned())),
        "calls: {cl:?}"
    );
}

// ── Dart ───────────────────────────────────────────────────────────────
#[test]
fn dart_symbols() {
    let c = chunker();
    let src = "class Point {\n  final int x;\n  Point(this.x);\n  int helper() => x;\n}\n";
    let a = anchors(&c, src, "dart");
    assert!(a.contains(&"class Point".to_owned()), "anchors: {a:?}");
    assert!(
        a.iter()
            .any(|s| s.starts_with("fn ") && s.ends_with("helper")),
        "method anchor: {a:?}"
    );
}

// ── Erlang ─────────────────────────────────────────────────────────────
#[test]
fn erlang_symbols_and_calls() {
    let c = chunker();
    let src = "helper(X) -> X.\nrun(X) -> helper(X).\n";
    let a = anchors(&c, src, "erl");
    assert!(a.contains(&"fn helper".to_owned()), "anchors: {a:?}");
    assert!(a.contains(&"fn run".to_owned()), "anchors: {a:?}");
    let cl = calls(&c, src, "erl", &["helper"]);
    assert!(
        cl.contains(&("run".to_owned(), "helper".to_owned())),
        "calls: {cl:?}"
    );
}

// ── OCaml ──────────────────────────────────────────────────────────────
#[test]
fn ocaml_symbols() {
    let c = chunker();
    let src = "let helper x = x\nlet run x = helper x\n";
    let a = anchors(&c, src, "ml");
    assert!(a.contains(&"fn helper".to_owned()), "anchors: {a:?}");
    assert!(a.contains(&"fn run".to_owned()), "anchors: {a:?}");
}

// ── Haskell ────────────────────────────────────────────────────────────
#[test]
fn haskell_symbols() {
    let c = chunker();
    let src = "helper :: Int -> Int\nhelper x = x\nrun x = helper x\n";
    let a = anchors(&c, src, "hs");
    assert!(a.contains(&"fn helper".to_owned()), "anchors: {a:?}");
    assert!(a.contains(&"fn run".to_owned()), "anchors: {a:?}");
}

/// Registration composes with (never clobbers) the built-in grammars, and
/// repeated registration is idempotent.
#[test]
fn registration_composes_with_builtins() {
    let mut c = TreeSitterChunker::new().expect("chunker");
    assert!(!c.supports("lua"));
    engram_lang::register_all(&mut c);
    assert!(c.supports("lua") && c.supports("jl") && c.supports("zig"));
    assert!(
        c.supports("rs") && c.supports("ts") && c.supports("go"),
        "builtins intact"
    );
    let before = anchors(&c, "local function helper(x) return x end", "lua");
    engram_lang::register_all(&mut c);
    let after = anchors(&c, "local function helper(x) return x end", "lua");
    assert_eq!(before, after, "idempotent");
}
