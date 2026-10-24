//! Receiver-chain evaluation (receiver-aware call resolution, plan v6 §2).
//!
//! Walks `call_expression` nodes and resolves each receiver chain through
//! the explicit state machine:
//!
//! ```text
//! Typed(P) ──link m──▶ emit P::m (if anchor count ≤ 1, else DROP)
//!     │ next = repo.return_of(P::m)
//!     ├── Returns(U) ─▶ Typed(U)
//!     └── Ambiguous/absent ─▶ Unknown{origin}   (ABSORBING)
//! Unknown{Var(v)} ──first link only──▶ emit `v.m` (non-generic) / drop (generic)
//! Unknown{Expression} ─▶ drop
//! ```
//!
//! Rust constructor roots (`Type::new(...)`) consult the repo return table
//! (no Self assumption); Java/TS `new Type()` establishes `Type` by language
//! semantics. Field receivers (`self.config.m()`) type through the per-owner
//! struct field map. Argument closures use their own scope (no state leak).

use crate::noise::is_generic_method;
use crate::parser::receiver_env::{BindType, FileContext};
use crate::parser::repo_context::{Canonical, DeclEvidence, RepoContext};

/// One evaluated call site: (line, reference) — the same downstream shape
/// the extractor has always consumed.
pub type EvaluatedSite = (usize, String);

/// Receiver state carried across chain links. `Unknown` is absorbing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// A canonicalized (possibly path-qualified) owner.
    Typed(String),
    /// Receiver is a variable of unknown type (honest hint retained).
    UnknownVar(String),
    /// Receiver is an expression result of unknown type (no honest hint).
    UnknownExpression,
}

/// Evaluates all call sites in a file. `ext` selects language behavior
/// (`rs` vs TS/JS/Java constructor semantics).
pub fn evaluate_call_sites(
    root: tree_sitter::Node,
    source: &[u8],
    ext: &str,
    fc: &FileContext,
    repo: &RepoContext,
) -> Vec<EvaluatedSite> {
    let mut sites = Vec::new();
    walk_calls(root, source, ext, fc, repo, &mut sites);
    sites
}

fn walk_calls(
    node: tree_sitter::Node,
    source: &[u8],
    ext: &str,
    fc: &FileContext,
    repo: &RepoContext,
    sites: &mut Vec<EvaluatedSite>,
) {
    let kind = node.kind();
    // scan-reliability AC5: JSX elements are declarative component usage.
    if kind == "jsx_opening_element" || kind == "jsx_self_closing_element" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = node_text(name_node, source);
            if name.chars().next().is_some_and(char::is_uppercase) {
                sites.push((node.start_position().row, name.to_owned()));
                return;
            }
        }
    }
    if matches!(
        kind,
        "call_expression" | "method_invocation" | "function_call" | "call"
    ) {
        evaluate_call(node, source, ext, fc, repo, sites);
        // Argument subtrees (including closures) are visited independently —
        // outer chain state never leaks into them.
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            walk_calls(child, source, ext, fc, repo, sites);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_calls(child, source, ext, fc, repo, sites);
    }
}

fn node_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// Evaluates one call expression, possibly emitting multiple sites (one per
/// method link in the receiver chain that survives the state machine).
fn evaluate_call(
    node: tree_sitter::Node,
    source: &[u8],
    ext: &str,
    fc: &FileContext,
    repo: &RepoContext,
    sites: &mut Vec<EvaluatedSite>,
) {
    let line = node.start_position().row;
    let Some(func) = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.named_child(0))
    else {
        return;
    };
    match func.kind() {
        // Bare call `foo()` — free function; reference as-written.
        "identifier" | "plain_identifier" | "self" => {
            sites.push((line, node_text(func, source).to_owned()));
        }
        // Scoped call `Type::new(...)` / `module::fn(...)` — qualified
        // reference as-written; `Type::new`-style ctors also seed state.
        "scoped_identifier" | "scoped_type_identifier" | "nested_identifier" => {
            let text = node_text(func, source);
            sites.push((line, text.to_owned()));
        }
        // Method call `recv.method(...)` — walk the receiver chain.
        "field_expression"
        | "member_expression"
        | "dot_index_expression"
        | "method_index_expression" => {
            let mut methods: Vec<&str> = Vec::new();
            let mut receiver = func;
            loop {
                if let Some(field) = receiver
                    .child_by_field_name("field")
                    .or_else(|| receiver.child_by_field_name("property"))
                    .or_else(|| receiver.child_by_field_name("method"))
                {
                    methods.push(node_text(field, source));
                    match receiver
                        .child_by_field_name("object")
                        .or_else(|| receiver.child_by_field_name("table"))
                        .or_else(|| receiver.named_child(0))
                    {
                        Some(obj)
                            if matches!(
                                obj.kind(),
                                "field_expression"
                                    | "member_expression"
                                    | "dot_index_expression"
                                    | "method_index_expression"
                            ) =>
                        {
                            receiver = obj;
                        }
                        Some(obj) => {
                            evaluate_chain(obj, &methods, line, source, ext, fc, repo, sites);
                            return;
                        }
                        None => return,
                    }
                } else {
                    return;
                }
            }
        }
        _ => {
            // Unknown callee shape — keep the identifier-ish text if simple.
            let text = node_text(func, source);
            if !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == ':' || c == '.')
            {
                sites.push((line, text.to_owned()));
            }
        }
    }
}

/// Evaluates a chain root object with `methods` (outermost first):
/// `methods[0]` is the CALLED method; `methods[1..]` are FIELD hops
/// (`self.config.line_buffer()` → method `line_buffer`, hop `config`).
#[allow(clippy::too_many_arguments)]
fn evaluate_chain(
    root_obj: tree_sitter::Node,
    methods: &[&str],
    line: usize,
    source: &[u8],
    ext: &str,
    fc: &FileContext,
    repo: &RepoContext,
    sites: &mut Vec<EvaluatedSite>,
) {
    let mut state = root_state(root_obj, source, ext, fc, repo, sites);
    // FIELD hops in application order (innermost first).
    for hop in methods.iter().skip(1).rev() {
        state = match &state {
            State::Typed(p) => {
                let short = p.rsplit("::").next().unwrap_or(p);
                match field_type(fc, repo, short, hop) {
                    Some(t) => State::Typed(t),
                    // Unknown-typed field directly on the root: retain its
                    // name as the honest var-style hint.
                    None => State::UnknownVar((*hop).to_owned()),
                }
            }
            // Unknown receiver with further hops: the method's receiver is
            // an expression (var.field) — no single honest hint.
            _ => State::UnknownExpression,
        };
    }
    // The called method (methods[0]).
    if let Some(method) = methods.first() {
        match &state {
            State::Typed(p) => {
                let anchor = format!("{p}::{method}");
                if repo.anchor_count(&anchor) > 1 {
                    // Collapsed trait anchor — drop, stop propagating.
                    return;
                }
                sites.push((line, anchor));
                // Terminal state unused here unless this chain is itself a
                // receiver (inner_call_state handles that replay).
            }
            State::UnknownVar(v) => {
                if !is_generic_method(method) {
                    sites.push((line, format!("{v}.{method}")));
                }
            }
            State::UnknownExpression => {}
        }
    }
}

/// Field type lookup: file-local struct map first, then repo-wide.
fn field_type(fc: &FileContext, repo: &RepoContext, owner: &str, field: &str) -> Option<String> {
    if let Some(t) = fc.field_type_of(owner, field) {
        return Some(t.to_owned());
    }
    repo.field_type_of(owner, field).map(str::to_owned)
}

/// Computes the initial state for a chain root object, possibly consuming
/// nested constructor calls (which emit their own sites).
fn root_state(
    root_obj: tree_sitter::Node,
    source: &[u8],
    ext: &str,
    fc: &FileContext,
    repo: &RepoContext,
    sites: &mut Vec<EvaluatedSite>,
) -> State {
    match root_obj.kind() {
        "identifier" | "plain_identifier" | "self" => {
            let name = node_text(root_obj, source);
            if name == "self" || name == "this" {
                // self.x() — the impl self-type (anchor-collapsed guard
                // applies in the Typed branch).
                return match fc.self_type_at(root_obj.start_byte()) {
                    Some(t) => canonical_state(t, fc, repo),
                    None => State::UnknownExpression,
                };
            }
            match fc
                .env
                .as_ref()
                .and_then(|e| e.lookup(name, root_obj.start_byte()))
            {
                Some(BindType::Known(t)) => canonical_state(t, fc, repo),
                Some(BindType::Unknown) => State::UnknownVar(name.to_owned()),
                None => State::UnknownVar(name.to_owned()),
            }
        }
        // Nested call: a.b().c() — evaluate the inner chain first; its
        // terminal state seeds this one.
        "call_expression" | "method_invocation" => {
            evaluate_call(root_obj, source, ext, fc, repo, sites);
            // Recompute the inner chain's terminal state (evaluate_call
            // emitted its sites; we need its final state — replay).
            inner_call_state(root_obj, source, ext, fc, repo)
        }
        "scoped_identifier" => {
            // `Type::new(...).method()` — constructor root (Rust).
            let text = node_text(root_obj, source);
            if let Some((owner, method)) = text.rsplit_once("::") {
                if matches!(method, "new" | "default") && !owner.contains("::") {
                    if let Some(ty) = terminal(owner) {
                        return seed_ctor_state(&ty, fc, repo);
                    }
                }
            }
            State::UnknownExpression
        }
        "new_expression" => {
            // Java/TS `new Type()` — language constructor semantics.
            root_obj
                .child_by_field_name("constructor")
                .or_else(|| root_obj.named_child(0))
                .and_then(|c| terminal(node_text(c, source)))
                .map(|t| canonical_state(&t, fc, repo))
                .unwrap_or(State::UnknownExpression)
        }
        _ => State::UnknownExpression,
    }
}

/// Terminal state of an inner call (replays the chain logic for its last
/// emitted link without re-emitting).
fn inner_call_state(
    node: tree_sitter::Node,
    source: &[u8],
    ext: &str,
    fc: &FileContext,
    repo: &RepoContext,
) -> State {
    let Some(func) = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.named_child(0))
    else {
        return State::UnknownExpression;
    };
    match func.kind() {
        "field_expression"
        | "member_expression"
        | "dot_index_expression"
        | "method_index_expression" => {
            let mut methods: Vec<&str> = Vec::new();
            let mut receiver = func;
            loop {
                if let Some(field) = receiver
                    .child_by_field_name("field")
                    .or_else(|| receiver.child_by_field_name("property"))
                    .or_else(|| receiver.child_by_field_name("method"))
                {
                    methods.push(node_text(field, source));
                    match receiver
                        .child_by_field_name("object")
                        .or_else(|| receiver.child_by_field_name("table"))
                        .or_else(|| receiver.named_child(0))
                    {
                        Some(obj)
                            if matches!(
                                obj.kind(),
                                "field_expression"
                                    | "member_expression"
                                    | "dot_index_expression"
                                    | "method_index_expression"
                            ) =>
                        {
                            receiver = obj;
                        }
                        Some(obj) => {
                            // Replay: root state, then field hops, then the
                            // method's return-table transition.
                            let mut state = root_state(obj, source, ext, fc, repo, &mut Vec::new());
                            for hop in methods.iter().skip(1).rev() {
                                state = match &state {
                                    State::Typed(p) => {
                                        let short = p.rsplit("::").next().unwrap_or(p);
                                        match field_type(fc, repo, short, hop) {
                                            Some(t) => State::Typed(t),
                                            None => State::UnknownVar((*hop).to_owned()),
                                        }
                                    }
                                    _ => State::UnknownExpression,
                                };
                            }
                            if let Some(method) = methods.first() {
                                state = match &state {
                                    State::Typed(p) => {
                                        let anchor = format!("{p}::{method}");
                                        match repo.return_of(&anchor) {
                                            Some(DeclEvidence::Returns(u)) => {
                                                State::Typed(u.clone())
                                            }
                                            _ => State::UnknownExpression,
                                        }
                                    }
                                    _ => State::UnknownExpression,
                                };
                            }
                            return state;
                        }
                        None => return State::UnknownExpression,
                    }
                } else {
                    return State::UnknownExpression;
                }
            }
        }
        "scoped_identifier" => {
            let text = node_text(func, source);
            if ext == "rs" {
                if let Some((owner, method)) = text.rsplit_once("::") {
                    if matches!(method, "new" | "default") && !owner.contains("::") {
                        if let Some(ty) = terminal(owner) {
                            return seed_ctor_state(&ty, fc, repo);
                        }
                    }
                }
            }
            State::UnknownExpression
        }
        "new_expression" => root_state(func, source, ext, fc, repo, &mut Vec::new()),
        _ => State::UnknownExpression,
    }
}

/// Canonicalizes an owner name to a `Typed` state (`Canonical::Short`
/// staying short is fine — anchors are short at top level).
fn canonical_state(name: &str, fc: &FileContext, repo: &RepoContext) -> State {
    match fc.canonical_owner(repo, name) {
        Canonical::Qualified(p) => State::Typed(p),
        Canonical::Short(s) => State::Typed(s),
        Canonical::Unknown => State::UnknownExpression,
    }
}

/// Rust constructor seeding: the table decides; absent/Ambiguous → Unknown
/// (no Self assumption). The constructor call itself was already emitted as
/// a scoped reference.
fn seed_ctor_state(ty: &str, fc: &FileContext, repo: &RepoContext) -> State {
    match fc.canonical_owner(repo, ty) {
        Canonical::Qualified(p) => match repo
            .return_of(&format!("{p}::new"))
            .or_else(|| repo.return_of(&format!("{p}::default")))
        {
            Some(DeclEvidence::Returns(u)) => State::Typed(u.clone()),
            _ => State::UnknownExpression,
        },
        Canonical::Short(s) => match repo
            .return_of(&format!("{s}::new"))
            .or_else(|| repo.return_of(&format!("{s}::default")))
        {
            Some(DeclEvidence::Returns(u)) => State::Typed(u.clone()),
            _ => State::UnknownExpression,
        },
        Canonical::Unknown => State::UnknownExpression,
    }
}

/// Terminal segment of a possibly module-qualified owner (`a::B::C` → `C`).
fn terminal(text: &str) -> Option<String> {
    let seg = text.rsplit("::").next().unwrap_or(text).trim();
    if !seg.is_empty()
        && seg.chars().next().is_some_and(char::is_uppercase)
        && seg.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        Some(seg.to_owned())
    } else {
        None
    }
}
