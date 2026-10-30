//! Per-file receiver environments (receiver-aware call resolution, plan v6).
//!
//! A [`ReceiverEnv`] is a position-aware scope TREE capturing variable →
//! type bindings for one file: function parameters, let-initializer typing
//! (through repo return evidence), type annotations, closure captures,
//! nested-function barriers, and shadowing (`Unknown` bindings hide outer
//! known bindings). [`FileContext`] additionally carries per-struct field
//! maps, import alias maps, and impl self-type ranges.
//!
//! Pure: built from a parsed tree + source text; no I/O, no storage.

use std::collections::HashMap;

use crate::parser::repo_context::{Canonical, RepoContext};

/// Scope kind — governs outward lookup behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Top-level or nested function item. Nested ones act as capture
    /// BARRIERS (Rust nested `fn` items capture nothing).
    Function,
    /// Closure — may capture outer bindings (lookup continues outward).
    Closure,
    /// Plain block (`{ let … }`) — lexical child scope.
    Block,
}

/// A variable → type binding at a byte position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindType {
    Known(String),
    Unknown,
}

/// One scope node in the tree.
#[derive(Debug, Clone)]
pub struct ScopeNode {
    pub kind: ScopeKind,
    pub start_byte: usize,
    pub end_byte: usize,
    /// Position-ordered bindings within this scope.
    pub bindings: Vec<Binding>,
    pub children: Vec<ScopeNode>,
}

/// A single binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub type_of: BindType,
    pub at_byte: usize,
    /// Raw initializer text, for the position-dependent post-pass.
    pub init: Option<String>,
}

impl ScopeNode {
    /// Innermost-scope lookup for `name` used at `byte`, honoring:
    /// position (latest binding `at_byte <= byte` wins within a scope),
    /// `Unknown` shadowing, closure capture (continue outward), and the
    /// nested-function capture barrier (a non-top-level `Function` scope
    /// does not consult outward).
    pub fn lookup(&self, name: &str, byte: usize) -> Option<&BindType> {
        if let Some(inner) = self
            .children
            .iter()
            .filter(|c| byte >= c.start_byte && byte <= c.end_byte)
            .max_by_key(|c| c.start_byte)
        {
            if let Some(found) = inner.lookup(name, byte) {
                return Some(found);
            }
            // If the inner scope was a nested function, it is a capture
            // barrier: stop rather than consulting this scope's bindings.
            if inner.kind == ScopeKind::Function {
                return None;
            }
        }
        // Latest binding at or before the use position.
        self.bindings
            .iter()
            .rev()
            .find(|b| b.name == name && b.at_byte <= byte)
            .map(|b| &b.type_of)
    }
}

/// Per-file environment (scope tree root is the file scope).
#[derive(Debug, Clone)]
pub struct ReceiverEnv {
    root: ScopeNode,
}

impl ReceiverEnv {
    /// Lookup `name` at `byte` anywhere in the file scope tree.
    pub fn lookup(&self, name: &str, byte: usize) -> Option<&BindType> {
        self.root.lookup(name, byte)
    }
}

/// One `impl` block's byte range and its self type (generic base: `UTF8`).
#[derive(Debug, Clone)]
pub struct ImplRange {
    pub start_byte: usize,
    pub end_byte: usize,
    /// Short self type name (impl target, generic args stripped).
    pub self_type: String,
}

/// Per-file context: scope env plus struct fields, aliases, impl ranges.
#[derive(Debug, Clone, Default)]
pub struct FileContext {
    pub env: Option<ReceiverEnv>,
    /// Short struct name → field → type (short names — source-local).
    pub structs: HashMap<String, HashMap<String, String>>,
    /// Alias → qualified path (`use remote::Product as RP`, `use crate::store as Store`).
    pub aliases: HashMap<String, String>,
    /// Impl self-type ranges (byte ranges, innermost wins).
    pub impls: Vec<ImplRange>,
}

impl FileContext {
    /// Empty context (unsupported languages / parse failures).
    pub fn empty() -> Self {
        Self::default()
    }

    /// The self type of the innermost impl containing `byte`.
    pub fn self_type_at(&self, byte: usize) -> Option<&str> {
        self.impls
            .iter()
            .filter(|i| byte >= i.start_byte && byte <= i.end_byte)
            .max_by_key(|i| i.start_byte)
            .map(|i| i.self_type.as_str())
    }

    /// Field type lookup: `self.field` receiver inside `impl T` resolves
    /// through T's field map.
    pub fn field_type_of(&self, self_type: &str, field: &str) -> Option<&str> {
        self.structs.get(self_type)?.get(field).map(|s| s.as_str())
    }

    /// Canonicalizes an owner name using the file's aliases + repo owners.
    pub fn canonical_owner(&self, repo: &RepoContext, name: &str) -> Canonical {
        repo.canonical_owner(name, &self.aliases)
    }
}

/// Builds a [`FileContext`] from a parsed tree (rs / ts / js / java).
///
/// Language-generic on stable tree-sitter node kinds:
/// - rs: `function_item`, `closure_expression`, `block`, `let_statement`,
///   `struct_item`, `impl_item`, `use_declaration`, `mod_item`.
/// - ts/js: `function_declaration`/`method_definition`, `arrow_function`,
///   `statement_block`, `lexical_declaration`/`variable_declaration`,
///   `class_declaration`, `import_statement`.
/// - java: `method_declaration`, `lambda_expression`, `block`,
///   `local_variable_declaration`, `class_declaration`.
pub fn build_file_context(
    tree: &tree_sitter::Tree,
    source: &str,
    ext: &str,
    repo: &RepoContext,
) -> FileContext {
    let mut ctx = FileContext::empty();
    let bytes = source.as_bytes();
    REPO.with(|r| *r.borrow_mut() = repo.clone());
    let root_scope = ScopeNode {
        kind: ScopeKind::Block,
        start_byte: 0,
        end_byte: bytes.len(),
        bindings: Vec::new(),
        children: Vec::new(),
    };
    let mut walker = Walker {
        source: bytes,
        ext,
        repo,
        ctx: &mut ctx,
        is_rs: ext == "rs",
    };
    let mut root_scope = walker.walk_scope(tree.root_node(), root_scope, false);
    // Position-dependent post-pass: type `let x = var.method(...)` and
    // `let x = var` initializers using earlier bindings + repo evidence.
    resolve_inits(&mut root_scope);
    ctx.env = Some(ReceiverEnv { root: root_scope });
    ctx
}

thread_local! {
    /// Repo evidence for the init post-pass (set per build call).
    static REPO: std::cell::RefCell<RepoContext> = std::cell::RefCell::new(RepoContext::empty());
}

enum InitShape {
    VarMethod(String, String),
    VarCopy(String),
    Other,
}

fn classify_init(init: &str) -> InitShape {
    let t = init.trim();
    if let Some(paren) = t.find('(') {
        let head = &t[..paren];
        if let Some(dot) = head.rfind('.') {
            let var = head[..dot].trim();
            let method = head[dot + 1..].trim();
            let var_ok = !var.is_empty() && var.chars().all(|c| c.is_alphanumeric() || c == '_');
            let m_ok =
                !method.is_empty() && method.chars().all(|c| c.is_alphanumeric() || c == '_');
            if var_ok && m_ok && !method.chars().next().is_some_and(char::is_uppercase) {
                return InitShape::VarMethod(var.to_owned(), method.to_owned());
            }
        }
    }
    if !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return InitShape::VarCopy(t.to_owned());
    }
    InitShape::Other
}

/// Upgrades Unknown bindings whose initializer is `var.method(...)` or a
/// bare `var` copy through earlier bindings + the repo return table.
/// Lookups use the ROOT scope so outer bindings (params, enclosing lets) are
/// visible; mutation follows the recorded child path.
fn resolve_inits(root: &mut ScopeNode) {
    // Phase 1: collect (path, index) for every binding with an init.
    let mut targets: Vec<(Vec<usize>, usize)> = Vec::new();
    collect_init_targets(root, Vec::new(), &mut targets);
    // Phase 2: classify + resolve against the root env, then mutate.
    for (path, idx) in targets {
        let Some(scope) = scope_at(root, &path) else {
            continue;
        };
        let (at, init) = {
            let b = &scope.bindings[idx];
            (b.at_byte, b.init.clone())
        };
        let Some(init) = init else { continue };
        if scope.bindings[idx].type_of != BindType::Unknown {
            continue;
        }
        let typed = match classify_init(&init) {
            InitShape::VarMethod(var, method) => match root.lookup(&var, at) {
                Some(BindType::Known(t)) => {
                    let anchor = format!("{t}::{method}");
                    REPO.with(|r| match r.borrow().return_of(&anchor) {
                        Some(crate::parser::repo_context::DeclEvidence::Returns(u)) => {
                            Some(BindType::Known(u.clone()))
                        }
                        _ => None,
                    })
                }
                _ => None,
            },
            InitShape::VarCopy(var) => match root.lookup(&var, at) {
                Some(t @ BindType::Known(_)) => Some(t.clone()),
                _ => None,
            },
            InitShape::Other => None,
        };
        if let Some(t) = typed
            && let Some(scope) = scope_at(root, &path)
        {
            scope.bindings[idx].type_of = t;
        }
    }
}

fn collect_init_targets(scope: &ScopeNode, path: Vec<usize>, out: &mut Vec<(Vec<usize>, usize)>) {
    for (i, b) in scope.bindings.iter().enumerate() {
        if b.init.is_some() {
            out.push((path.clone(), i));
        }
    }
    for (ci, child) in scope.children.iter().enumerate() {
        let mut p = path.clone();
        p.push(ci);
        collect_init_targets(child, p, out);
    }
}

fn scope_at<'s>(root: &'s mut ScopeNode, path: &[usize]) -> Option<&'s mut ScopeNode> {
    let mut cur = root;
    for idx in path {
        cur = cur.children.get_mut(*idx)?;
    }
    Some(cur)
}

/// Terminal type extraction from a type-node text: strips refs, lifetimes,
/// generics; keeps module paths (`remote::Product` → `remote::Product`).
fn terminal_type(text: &str) -> Option<String> {
    let mut t = text.trim();
    loop {
        if let Some(rest) = t.strip_prefix('&') {
            t = rest
                .trim_start()
                .strip_prefix("mut ")
                .map(str::trim_start)
                .unwrap_or(rest.trim_start());
            continue;
        }
        break;
    }
    let t = t.strip_prefix("crate::").unwrap_or(t);
    // Strip generic args.
    let t = match t.find('<') {
        Some(i) => &t[..i],
        None => t,
    };
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    // Simple identifier path of alnum/_/: segments, terminal must be a
    // plausible type name (PascalCase terminal; primitives excluded).
    let last = t.rsplit("::").next().unwrap_or(t);
    const PRIMITIVES: &[&str] = &[
        "str",
        "String",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "isize",
        "u8",
        "u16",
        "u32",
        "u64",
        "u128",
        "usize",
        "f32",
        "f64",
        "bool",
        "char",
        "string",
        "number",
        "boolean",
        "any",
        "unknown",
        "never",
        "object",
        "symbol",
        "bigint",
        "void",
        "null",
        "undefined",
        "byte",
        "int",
        "long",
        "short",
        "float",
        "double",
        "Vec",
        "Option",
        "Result",
        "Box",
        "Rc",
        "Arc",
        "Self",
        "Self::Output",
    ];
    if t.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        && last.chars().next().is_some_and(char::is_uppercase)
        && !PRIMITIVES.contains(&last)
    {
        Some(t.to_owned())
    } else {
        None
    }
}

/// Parses Rust `use` declarations into alias → target pairs (type-symbol and
/// module aliases, grouped forms included). Returns (alias, target) pairs;
/// plain (non-aliased) imports return their last segment as an identity
/// alias only when the item is PascalCase (a type import).
fn parse_use_aliases(text: &str) -> Vec<(String, String)> {
    let t = text.trim().trim_start_matches("pub ").trim();
    let Some(rest) = t.strip_prefix("use ") else {
        return Vec::new();
    };
    let body = rest.trim().trim_end_matches(';');
    // Split the module prefix from a possibly-braced item list.
    if let (Some(open), true) = (body.find('{'), body.ends_with('}')) {
        let module = &body[..open];
        let items = &body[open + 1..body.len() - 1];
        let mut out = Vec::new();
        for item in items.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            if let Some((orig, alias)) = item.split_once(" as ") {
                out.push((alias.trim().to_owned(), format!("{module}{}", orig.trim())));
            }
            // Plain braced items (`use remote::{Product}`) create NO alias:
            // the imported name already matches the entity's anchor key
            // (anchors are module-prefixed only by IN-FILE nesting, never by
            // crate/import paths — a path-qualified alias would miss the
            // repo table and wrongly degrade typed receivers).
        }
        return out;
    }
    // Single path form.
    let body = body.trim();
    if let Some((orig, alias)) = body.rsplit_once(" as ") {
        return vec![(alias.trim().to_owned(), orig.trim().to_owned())];
    }
    // Plain single-path imports (`use ignore::WalkBuilder;`) create NO
    // alias: the imported name IS the anchor key (see the braced-form note).
    // Only explicit `as` aliases rewrite.
    Vec::new()
}

struct Walker<'a> {
    source: &'a [u8],
    ext: &'a str,
    repo: &'a RepoContext,
    ctx: &'a mut FileContext,
    is_rs: bool,
}

impl<'a> Walker<'a> {
    fn text<'n>(&self, node: tree_sitter::Node<'n>) -> &'a str {
        node.utf8_text(self.source).unwrap_or("")
    }

    /// Walks `node` collecting declarations into `scope`, descending into
    /// nested scopes. `in_fn` marks whether we're inside a function (nested
    /// function items become capture barriers via `ScopeKind::Function`).
    fn walk_scope(
        &mut self,
        node: tree_sitter::Node,
        mut scope: ScopeNode,
        in_fn: bool,
    ) -> ScopeNode {
        let kind = node.kind();
        // New nested scope boundaries.
        if self.is_scope_kind(kind) {
            let nested_kind = if matches!(
                kind,
                "closure_expression" | "arrow_function" | "lambda_expression"
            ) {
                ScopeKind::Closure
            } else if self.is_function_kind(kind) {
                // Function scope. Nested ones (inside another function) act
                // as capture barriers in `ScopeNode::lookup`.
                ScopeKind::Function
            } else {
                ScopeKind::Block
            };
            let _ = in_fn;
            let mut nested = ScopeNode {
                kind: nested_kind,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                bindings: Vec::new(),
                children: Vec::new(),
            };
            // Function-like scopes bind their parameters.
            if nested_kind != ScopeKind::Block {
                if let Some(params) = self.parameters_node(node) {
                    self.bind_parameters(params, &mut nested);
                } else if nested_kind == ScopeKind::Closure {
                    // Rust closure `|x| …` — every param is Unknown.
                    if let Some(params) = node.child_by_field_name("parameters") {
                        let mut cursor = params.walk();
                        for p in params.named_children(&mut cursor) {
                            if let Some(name) = self.simple_name(p) {
                                nested.bindings.push(Binding {
                                    name,
                                    type_of: BindType::Unknown,
                                    at_byte: p.start_byte(),
                                    init: None,
                                });
                            }
                        }
                    }
                }
            }
            let mut cursor = node.walk();
            let children: Vec<_> = node.named_children(&mut cursor).collect();
            for child in children {
                nested = self.walk_scope(child, nested, true);
            }
            scope.children.push(nested);
            return scope;
        }

        match kind {
            // let / var declarations bind names in the CURRENT scope.
            "let_declaration" | "let_statement" if self.is_rs => self.bind_let(node, &mut scope),
            "lexical_declaration" | "variable_declaration" | "variable_declarator" => {
                self.bind_var_decl(node, &mut scope)
            }
            "local_variable_declaration" => self.bind_var_decl(node, &mut scope),
            // File-level metadata.
            "struct_item" | "class_declaration" | "class_specifier" => self.collect_struct(node),
            "impl_item" => self.collect_impl(node),
            "use_declaration" => {
                for (alias, target) in parse_use_aliases(self.text(node)) {
                    self.ctx.aliases.insert(alias, target);
                }
            }
            "mod_item" => {
                // Not forwarded to RepoContext here — the repo-level builder
                // collects module declarations (this walker is per-file env).
            }
            _ => {}
        }

        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children {
            scope = self.walk_scope(child, scope, in_fn);
        }
        scope
    }

    fn is_scope_kind(&self, kind: &str) -> bool {
        self.is_function_kind(kind)
            || matches!(
                kind,
                "closure_expression" | "arrow_function" | "lambda_expression"
            )
            || (kind == "block" && self.is_rs)
            || (kind == "statement_block" && !self.is_rs)
    }

    fn is_function_kind(&self, kind: &str) -> bool {
        if self.is_rs {
            kind == "function_item"
        } else if matches!(self.ext, "java") {
            kind == "method_declaration" || kind == "constructor_declaration"
        } else {
            matches!(
                kind,
                "function_declaration" | "method_definition" | "function_expression" | "function"
            )
        }
    }

    fn parameters_node<'n>(&self, node: tree_sitter::Node<'n>) -> Option<tree_sitter::Node<'n>> {
        node.child_by_field_name("parameters")
    }

    fn simple_name(&self, node: tree_sitter::Node) -> Option<String> {
        let text = self.text(node);
        let t = text.trim();
        if !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_') {
            Some(t.to_owned())
        } else {
            None
        }
    }

    /// Binds function parameters from a `parameters` node.
    fn bind_parameters(&mut self, params: tree_sitter::Node, scope: &mut ScopeNode) {
        let mut cursor = params.walk();
        for p in params.named_children(&mut cursor) {
            // Rust: parameter { pattern?: identifier, type?: type node };
            // tree-sitter-rust ≥0.24 uses fieldless children
            // (identifier + type_identifier). TS: required_parameter.
            let (name_node, type_node) = if matches!(
                p.kind(),
                "parameter" | "formal_parameter" | "required_parameter" | "optional_parameter"
            ) {
                let name = p.child_by_field_name("pattern").or_else(|| {
                    let mut c = p.walk();
                    p.named_children(&mut c)
                        .find(|ch| ch.kind() == "identifier" || ch.kind() == "property_identifier")
                });
                let ty = p.child_by_field_name("type").or_else(|| {
                    let mut c = p.walk();
                    p.named_children(&mut c).find(|ch| {
                        matches!(
                            ch.kind(),
                            "type_identifier"
                                | "generic_type"
                                | "reference_type"
                                | "scoped_type_identifier"
                                | "primitive_type"
                        ) && ch.start_byte() > name.map(|n| n.start_byte()).unwrap_or(0)
                    })
                });
                (name, ty)
            } else {
                (Some(p), None)
            };
            let Some(name_node) = name_node else { continue };
            let Some(name) = self.simple_name(name_node) else {
                // Destructured patterns bind their sub-identifiers as Unknown.
                let mut cursor = name_node.walk();
                for sub in name_node.named_children(&mut cursor) {
                    if let Some(n) = self.simple_name(sub) {
                        scope.bindings.push(Binding {
                            name: n,
                            type_of: BindType::Unknown,
                            at_byte: sub.start_byte(),
                            init: None,
                        });
                    }
                }
                continue;
            };
            let type_of = type_node
                .and_then(|t| terminal_type(self.text(t)))
                .map(BindType::Known)
                .unwrap_or(BindType::Unknown);
            scope.bindings.push(Binding {
                name,
                type_of,
                at_byte: p.start_byte(),
                init: None,
            });
        }
    }

    /// Rust `let PATTERN [: TYPE] = INIT;`.
    fn bind_let(&mut self, node: tree_sitter::Node, scope: &mut ScopeNode) {
        let pattern = node.child_by_field_name("pattern").or_else(|| {
            node.named_children(&mut node.walk())
                .find(|ch| ch.kind() == "identifier")
        });
        let type_node = node.child_by_field_name("type").or_else(|| {
            node.named_children(&mut node.walk())
                .find(|ch| ch.kind() == "type_identifier")
        });
        let value = node.child_by_field_name("value").or_else(|| {
            let pat_end = pattern.map(|p| p.end_byte()).unwrap_or(0);
            node.named_children(&mut node.walk()).find(|ch| {
                ch.start_byte() >= pat_end
                    && ch.kind() != "identifier"
                    && ch.kind() != "type_identifier"
                    && !ch.kind().contains("comment")
            })
        });
        let Some(pattern) = pattern else { return };
        let Some(name) = self.simple_name(pattern) else {
            // Destructuring: bind each sub-identifier Unknown.
            let mut cursor = pattern.walk();
            for sub in pattern.named_children(&mut cursor) {
                if let Some(n) = self.simple_name(sub) {
                    scope.bindings.push(Binding {
                        name: n,
                        type_of: BindType::Unknown,
                        at_byte: sub.start_byte(),
                        init: None,
                    });
                }
            }
            return;
        };
        // Annotation wins; else evaluate the initializer through repo evidence.
        let type_of = type_node
            .and_then(|t| terminal_type(self.text(t)))
            .map(BindType::Known)
            .or_else(|| value.and_then(|v| self.type_of_init(v)))
            .unwrap_or(BindType::Unknown);
        let init = value.map(|v| self.text(v).to_owned());
        scope.bindings.push(Binding {
            name,
            type_of,
            at_byte: node.start_byte(),
            init,
        });
    }

    /// TS/JS/Java var declarations (`let x = new Type()`, `Type x = …`).
    fn bind_var_decl(&mut self, node: tree_sitter::Node, scope: &mut ScopeNode) {
        // Two shapes: declaration node with declarator children, or the
        // declarator itself.
        let declarators: Vec<tree_sitter::Node> = if node.kind().ends_with("declaration")
            && node.kind() != "lexical_declaration"
            && node.kind() != "variable_declaration"
            && node.kind() != "local_variable_declaration"
        {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        } else {
            vec![node]
        };
        for d in declarators {
            let name_node = d.child_by_field_name("name").or_else(|| d.named_child(0));
            let value = d.child_by_field_name("value");
            let type_node = d.child_by_field_name("type");
            let Some(name_node) = name_node else { continue };
            let Some(name) = self.simple_name(name_node) else {
                continue;
            };
            let type_of = type_node
                .and_then(|t| terminal_type(self.text(t)))
                .map(BindType::Known)
                .or_else(|| value.and_then(|v| self.type_of_init(v)))
                .unwrap_or(BindType::Unknown);
            let init = value.map(|v| self.text(v).to_owned());
            scope.bindings.push(Binding {
                name,
                type_of,
                at_byte: d.start_byte(),
                init,
            });
        }
    }

    /// Initializer typing: `Type::new()` (rs, via repo return evidence),
    /// `new Type()` (ts/java), or an identifier copy.
    fn type_of_init(&self, value: tree_sitter::Node) -> Option<BindType> {
        let kind = value.kind();
        if kind == "call_expression" {
            if let Some(func) = value.child_by_field_name("function") {
                let text = self.text(func);
                // `Type::new` / `Type::default` (rs).
                if let Some(pos) = text.rfind("::") {
                    let method = &text[pos + 2..];
                    let owner_raw = &text[..pos];
                    // TERMINAL SEGMENT of a module-qualified ctor path
                    // (`grep::searcher::SearcherBuilder` → `SearcherBuilder`);
                    // canonicalization resolves it back through repo owners.
                    let terminal = owner_raw.rsplit("::").next().unwrap_or(owner_raw);
                    let owner = terminal_type(terminal)?;
                    if matches!(method, "new" | "default") {
                        let canon = self.ctx.canonical_owner(self.repo, &owner);
                        let anchor = match &canon {
                            Canonical::Qualified(p) => format!("{p}::{method}"),
                            Canonical::Short(s) => format!("{s}::{method}"),
                            Canonical::Unknown => return Some(BindType::Unknown),
                        };
                        return Some(match self.repo.return_of(&anchor) {
                            Some(crate::parser::repo_context::DeclEvidence::Returns(t)) => {
                                BindType::Known(t.clone())
                            }
                            _ => BindType::Unknown,
                        });
                    }
                }
            }
            return Some(BindType::Unknown);
        }
        if kind == "new_expression" {
            // `new Type(...)`.
            if let Some(ctor) = value.child_by_field_name("constructor")
                && let Some(t) = terminal_type(self.text(ctor))
            {
                return Some(BindType::Known(t));
            }
            return Some(BindType::Unknown);
        }
        if kind == "identifier" {
            let name = self.text(value);
            // Copy typing is position-dependent; conservatively Unknown here
            // (the scope lookup at the use site handles var-to-var via env
            // only when bound — copies stay Unknown).
            let _ = name;
            return Some(BindType::Unknown);
        }
        Some(BindType::Unknown)
    }

    /// Collects struct/class fields into the per-file map (short names).
    fn collect_struct(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.text(n).trim().to_owned())
            .or_else(|| {
                // Java class_declaration name field; rs struct_item name.
                node.child_by_field_name("name")
                    .map(|n| self.text(n).trim().to_owned())
            });
        let Some(name) = name else { return };
        if name.is_empty() {
            return;
        }
        // Field list: rs `field_declaration_list`; ts/java class body.
        let body = node.child_by_field_name("body").or_else(|| {
            node.named_children(&mut node.walk())
                .find(|c| c.kind() == "field_declaration_list" || c.kind() == "class_body")
        });
        let Some(body) = body else { return };
        let mut fields: HashMap<String, String> = HashMap::new();
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "field_declaration"
                || child.kind() == "property_declaration"
                || child.kind() == "field_declaration_list"
            {
                let fname = child
                    .child_by_field_name("name")
                    .or_else(|| child.named_child(0))
                    .and_then(|n| self.simple_name(n));
                let ftype = child
                    .child_by_field_name("type")
                    .and_then(|t| terminal_type(self.text(t)));
                if let (Some(f), Some(t)) = (fname, ftype) {
                    fields.insert(f, t);
                }
            }
        }
        if !fields.is_empty() {
            self.ctx.structs.insert(name, fields);
        }
    }

    /// Collects `impl` self-type ranges (generic base via last identifier).
    fn collect_impl(&mut self, node: tree_sitter::Node) {
        // `impl Trait for Type` → the `type` field; `impl Type` → `type`.
        let target = node
            .child_by_field_name("type")
            .map(|t| self.text(t).to_owned());
        let Some(target) = target else { return };
        let base = target
            .rsplit("::")
            .next()
            .unwrap_or(&target)
            .split('<')
            .next()
            .unwrap_or(&target)
            .trim()
            .trim_start_matches('&')
            .trim_end_matches('>')
            .to_owned();
        let base = base.split_whitespace().last().unwrap_or(&base).to_owned();
        if base.is_empty() || !base.chars().next().is_some_and(char::is_uppercase) {
            return;
        }
        self.ctx.impls.push(ImplRange {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            self_type: base,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::repo_context::{RepoContextBuilder, ReturnEvidence};

    pub(crate) fn parse(text: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("rust grammar");
        parser.parse(text, None).expect("parse")
    }

    fn ctx_with_returns() -> RepoContext {
        let mut b = RepoContextBuilder::default();
        b.declare_type("SearcherBuilder");
        b.declare_method(
            "SearcherBuilder::new",
            ReturnEvidence::Returns("SearcherBuilder".into()),
        );
        b.declare_method(
            "SearcherBuilder::build",
            ReturnEvidence::Returns("Searcher".into()),
        );
        b.declare_method(
            "LineBufferBuilder::new",
            ReturnEvidence::Returns("LineBufferBuilder".into()),
        );
        b.build()
    }

    #[test]
    fn scope_isolation_two_builders_same_name() {
        let repo = ctx_with_returns();
        let src = r#"
fn search_worker() {
    let mut builder = SearchWorkerBuilder::new();
    builder.build();
}
fn searcher() {
    let mut builder = SearcherBuilder::new();
    builder.build();
}
"#;
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        let env = fc.env.as_ref().unwrap();
        // Find `builder` uses per function by byte position.
        let first_use = src.find("builder.build").expect("first");
        let second_use = src.rfind("builder.build").expect("second");
        assert_eq!(
            env.lookup("builder", first_use),
            Some(&BindType::Unknown),
            "search_worker's builder: ctor not in repo table → Unknown"
        );
        assert_eq!(
            env.lookup("builder", second_use),
            Some(&BindType::Known("SearcherBuilder".into())),
            "searcher's builder types via repo return evidence"
        );
    }

    #[test]
    fn module_qualified_constructor_terminal_segment() {
        let repo = ctx_with_returns();
        let src = r#"
fn searcher() {
    let mut builder = grep::searcher::SearcherBuilder::new();
    builder.build();
}
"#;
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        let use_pos = src.find("builder.build").unwrap();
        assert_eq!(
            fc.env.as_ref().unwrap().lookup("builder", use_pos),
            Some(&BindType::Known("SearcherBuilder".into())),
            "module-qualified ctor path types by its TERMINAL segment"
        );
    }

    #[test]
    fn shadowing_unknown_hides_outer_known() {
        let repo = ctx_with_returns();
        let src = r#"
fn f(builder: SearcherBuilder) {
    builder.build();
    let builder = mystery();
    builder.build();
}
"#;
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        let env = fc.env.as_ref().unwrap();
        let uses: Vec<usize> = src.match_indices("builder.build").map(|(i, _)| i).collect();
        assert_eq!(uses.len(), 2);
        assert_eq!(
            env.lookup("builder", uses[0]),
            Some(&BindType::Known("SearcherBuilder".into()))
        );
        assert_eq!(
            env.lookup("builder", uses[1]),
            Some(&BindType::Unknown),
            "inner Unknown binding shadows outer Known"
        );
    }

    #[test]
    fn nested_fn_is_capture_barrier_but_closure_captures() {
        let repo = RepoContext::empty();
        let src = r#"
fn outer() {
    let v = SearcherBuilder::new();
    let inner = || { v.build(); };
    fn nested() { v.build(); }
}
"#;
        // v inside nested() wouldn't compile in Rust, but the TEST verifies
        // the barrier semantics: lookup from inside nested must NOT see v.
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        let env = fc.env.as_ref().unwrap();
        let closure_use = src.find("v.build").unwrap();
        assert!(
            env.lookup("v", closure_use).is_some(),
            "closure captures outer v"
        );
        let nested_pos = src.find("fn nested").unwrap();
        // Any lookup at the nested fn body position must stop at the barrier.
        let nested_body = nested_pos + 12;
        assert!(
            env.lookup("v", nested_body).is_none(),
            "nested fn is a capture barrier"
        );
    }

    #[test]
    fn impl_self_type_generic_base() {
        let repo = RepoContext::empty();
        let src = "impl<F: FnMut()> Sink for UTF8<F> { fn call(&self) { } }";
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        assert_eq!(fc.self_type_at(10), Some("UTF8"));
    }

    #[test]
    fn struct_fields_and_self_lookup() {
        let repo = RepoContext::empty();
        let src = r#"
struct SearcherBuilder { config: Config }
impl SearcherBuilder { fn build(&self) { } }
"#;
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        assert_eq!(
            fc.field_type_of("SearcherBuilder", "config"),
            Some("Config")
        );
        let build_pos = src.find("fn build").unwrap();
        assert_eq!(fc.self_type_at(build_pos), Some("SearcherBuilder"));
    }

    #[test]
    fn use_aliases_type_symbol_and_grouped() {
        let repo = RepoContext::empty();
        let src = "use remote::Product as RP;\nuse other::{Thing as T};\nfn f() {}";
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        assert_eq!(
            fc.aliases.get("RP").map(String::as_str),
            Some("remote::Product")
        );
        assert_eq!(
            fc.aliases.get("T").map(String::as_str),
            Some("other::Thing")
        );
    }

    #[test]
    fn let_rhs_typing_through_return_table() {
        let repo = ctx_with_returns();
        let src = r#"
fn f(b: SearcherBuilder) {
    let s = b.build();
    s.search_reader();
}
"#;
        let tree = parse(src);
        let fc = build_file_context(&tree, src, "rs", &repo);
        let use_pos = src.find("s.search_reader").unwrap();
        assert_eq!(
            fc.env.as_ref().unwrap().lookup("s", use_pos),
            Some(&BindType::Known("Searcher".into())),
            "let RHS typed through the COMPLETE return-table evaluation"
        );
    }
}
