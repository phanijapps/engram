//! Framework pattern resolvers (RFC-0020 Phase 2 starter set): HTTP routes
//! (Express, NestJS, FastAPI, Flask, Spring) and React event-handler
//! callbacks — deterministic line-pattern extraction, no LLM.
//!
//! Routes become `Endpoint`-kind entities named `METHOD /path` wired to
//! their handlers via `routes_to` edges; React `onClick={handler}` props
//! become `calls` edges from the enclosing component to the handler. The
//! extractor resolves handler names against the document's symbol table;
//! handlers defined in other files resolve cross-file like any reference.

/// One HTTP route fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFact {
    /// HTTP method, upper-case (`GET`).
    pub method: String,
    /// Route path as written (`/users/:id`).
    pub path: String,
    /// Handler function/method name as written.
    pub handler: String,
}

/// Framework facts for one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameworkFacts {
    pub routes: Vec<RouteFact>,
    /// (component, handler) React callback pairs (`onClick={handler}`).
    pub callbacks: Vec<(String, String)>,
    /// JVM-language annotation attachments (java/kt/scala/groovy): Spring
    /// wiring, DI markers, tests, composables — the typed graph surface for
    /// "which methods are @Transactional" questions.
    pub annotations: Vec<AnnotationFact>,
}

impl FrameworkFacts {
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty() && self.callbacks.is_empty() && self.annotations.is_empty()
    }
}

/// One annotation attachment (`@Transactional` on `find`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationFact {
    /// Annotation simple name (`Transactional`).
    pub annotation: String,
    /// Annotated declaration name as written.
    pub target: String,
}

/// Extract framework facts from source text for a file extension.
/// Unmapped extensions yield empty facts (never an error).
pub fn extract_frameworks(text: &str, ext: &str) -> FrameworkFacts {
    match ext {
        "ts" | "js" | "mjs" | "cjs" => extract_express_or_nestjs(text),
        "tsx" | "jsx" => {
            let mut facts = extract_express_or_nestjs(text);
            let (routes, cb) = extract_react_callbacks(text);
            facts.routes.extend(routes);
            facts.callbacks = cb;
            facts
        }
        "py" => extract_python_routes(text),
        "java" => extract_jvm_facts(text),
        "kt" | "kts" => extract_jvm_facts(text),
        "scala" => extract_jvm_facts(text),
        "groovy" | "gradle" => extract_jvm_facts(text),
        _ => FrameworkFacts::default(),
    }
}

/// First quoted string in a line (single or double quotes).
fn first_quoted(line: &str) -> Option<String> {
    let start = line.find(['\'', '"'])?;
    let quote = &line[start..start + 1];
    let rest = &line[start + 1..];
    let end = rest.find(quote.char_indices().next()?.1.to_string().as_str())?;
    Some(rest[..end].to_owned())
}

/// Identifier following a closing paren + comma (Express handler arg).
fn handler_after_paren(line: &str) -> Option<String> {
    // `app.get("/x", handler)` — take the identifier after the comma that
    // follows the path argument. Simple split: last identifier before `)` or `;`.
    let body = line.split('(').nth(1)?;
    let mut ident = String::new();
    let mut terminated = false;
    for ch in body.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else if ch == ')' {
            // The identifier must end at the call's closing paren — inline
            // arrow handlers (`(req, res) => …`) break their own parens and
            // are rejected rather than mis-attributed to a parameter name.
            terminated = true;
            break;
        } else if ch == ';' {
            break;
        } else {
            ident.clear();
        }
    }
    if ident.is_empty() || !terminated {
        None
    } else {
        Some(ident)
    }
}

/// Express (`app.get("/x", handler)`, `router.post('/x', h)`) and NestJS
/// (`@Get("x")` above a method) routes from TS/JS source.
fn extract_express_or_nestjs(text: &str) -> FrameworkFacts {
    let mut facts = FrameworkFacts::default();
    let mut pending_decorator: Option<(String, String)> = None; // (method, path)
    for raw_line in text.lines() {
        let line = raw_line.trim();
        // Express-style call: app.<method>(...) / router.<method>(...)
        if let Some((method, path, handler)) = express_route(line) {
            facts.routes.push(RouteFact {
                method,
                path,
                handler,
            });
            continue;
        }
        // NestJS decorator: @Get("path") / @All("path")
        if let Some(rest) = line.strip_prefix('@') {
            let (word, tail) = rest.split_once('(').unwrap_or((rest, ""));
            if matches!(
                word,
                "Get" | "Post" | "Put" | "Delete" | "Patch" | "All" | "Head" | "Options"
            ) && !tail.is_empty()
            {
                if let Some(path) = first_quoted(tail) {
                    pending_decorator = Some((word.to_uppercase(), path));
                    continue;
                }
            }
        }
        // Decorators/comments/blank lines after a route decorator do NOT
        // consume it — only a real declaration line is the handler
        // (stacked `@UseInterceptors`-style decorators are skipped).
        if line.starts_with('@') || line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some((method, path)) = pending_decorator.take() {
            if let Some(handler) = declaration_name(line) {
                facts.routes.push(RouteFact {
                    method,
                    path,
                    handler,
                });
            }
        }
    }
    facts
}

/// `app.get("/users", listUsers)` → (GET, /users, listUsers).
fn express_route(line: &str) -> Option<(String, String, String)> {
    let (receiver, rest) = line.split_once('.')?;
    if !matches!(receiver, "app" | "router" | "server" | "api") {
        return None;
    }
    let (method_word, tail) = rest.split_once('(')?;
    let method = match method_word {
        "get" | "post" | "put" | "delete" | "patch" | "all" | "head" | "options" => {
            method_word.to_uppercase()
        }
        "use" => return None, // middleware, not a route
        _ => return None,
    };
    let path = first_quoted(tail)?;
    let handler = handler_after_paren(line)?;
    // The handler must come after the path argument, not be it.
    if handler == path {
        return None;
    }
    Some((method, path, handler))
}

/// Handler name from a declaration line (`async listUsers(`, `def handler(`,
/// `public ResponseEntity<…> fetch(`).
fn declaration_name(line: &str) -> Option<String> {
    // Take the identifier immediately before the first '(' that follows
    // keywords — robust across languages for the starter set.
    let paren = line.find('(')?;
    let head = &line[..paren];
    let mut ident = String::new();
    for ch in head.chars().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.insert(0, ch);
        } else if !ident.is_empty() {
            break;
        }
    }
    if ident.is_empty()
        || matches!(
            ident.as_str(),
            "async" | "def" | "fn" | "public" | "private" | "protected" | "static" | "function"
        )
    {
        return None;
    }
    Some(ident)
}

/// FastAPI (`@app.get("/x")`) and Flask (`@app.route("/x", methods=[...])`)
/// decorators above `def handler`.
fn extract_python_routes(text: &str) -> FrameworkFacts {
    let mut facts = FrameworkFacts::default();
    let mut pending: Option<(String, String)> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix('@') {
            // FastAPI: app.get("/x") / router.post("/x")
            if let Some((receiver_call, tail)) = rest.split_once('(') {
                let Some(path) = first_quoted(tail) else {
                    continue;
                };
                let segments: Vec<&str> = receiver_call.split('.').collect();
                if segments.len() == 2 && matches!(segments[0], "app" | "router" | "api") {
                    let method = match segments[1] {
                        "route" => "GET".to_owned(), // Flask default
                        word => word.to_uppercase(),
                    };
                    pending = Some((method, path));
                    continue;
                }
            }
        }
        if line.starts_with('#') || line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some((method, path)) = pending.take() {
            if let Some(handler) = declaration_name(line) {
                facts.routes.push(RouteFact {
                    method,
                    path,
                    handler,
                });
            }
        }
    }
    facts
}

/// React event-handler callbacks: `onClick={handler}` (or `={() => handler()}`
/// inline — skipped, no stable name) inside a component. Pairs the closest
/// enclosing component declaration (heuristic: the last `function Name` /
/// `const Name = ` seen before the attribute) with the handler.
fn extract_react_callbacks(text: &str) -> (Vec<RouteFact>, Vec<(String, String)>) {
    let mut callbacks = Vec::new();
    let mut component = String::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        // Track the enclosing component (function Name / const Name =).
        if let Some(rest) = line.strip_prefix("function ") {
            if let Some(name) = rest.split(['(', ' ', '<']).next() {
                if !name.is_empty() {
                    component = name.to_owned();
                }
            }
        } else if let Some(rest) = line.strip_prefix("const ") {
            if let Some(name) = rest.split([' ', '=']).next() {
                if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    component = name.to_owned();
                }
            }
        }
        // on<Event>={handler} where handler is a bare identifier.
        for attr in [
            "onClick",
            "onChange",
            "onSubmit",
            "onError",
            "onSuccess",
            "onClose",
        ] {
            let Some((_, rest)) = line.split_once(&format!("{attr}={{")) else {
                continue;
            };
            let mut handler = String::new();
            for ch in rest.chars() {
                if ch.is_alphanumeric() || ch == '_' {
                    handler.push(ch);
                } else {
                    break;
                }
            }
            if !handler.is_empty() && !component.is_empty() {
                callbacks.push((component.clone(), handler));
            }
        }
    }
    (Vec::new(), callbacks)
}

/// JVM-language framework facts (java/kt/scala/groovy): Spring route
/// mappings WITH class-level `@RequestMapping` prefix composition, plus a
/// full annotation layer — every annotation attached to a declaration
/// becomes an [`AnnotationFact`] (Transactional, Service, Test, Composable).
/// Deterministic line-pattern extraction over the same discipline as
/// [`extract_spring_routes`]; declaration names resolve against the
/// document symbol table in the extractor.
pub fn extract_jvm_facts(text: &str) -> FrameworkFacts {
    let mut facts = FrameworkFacts::default();
    let mut pending_annotations: Vec<String> = Vec::new();
    let mut pending_route: Option<(String, String)> = None;
    // Class-level @RequestMapping("/api") prefixes every method route.
    let mut class_prefix: Option<String> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix('@') {
            let (word, tail) = rest.split_once('(').unwrap_or((rest, ""));
            let word = word.trim();
            if word.is_empty() {
                continue;
            }
            pending_annotations.push(word.to_owned());
            let method = match word {
                "GetMapping" => Some("GET"),
                "PostMapping" => Some("POST"),
                "PutMapping" => Some("PUT"),
                "DeleteMapping" => Some("DELETE"),
                "PatchMapping" => Some("PATCH"),
                "RequestMapping" => Some("GET"),
                _ => None,
            };
            if let Some(method) = method {
                let path = first_quoted(tail).unwrap_or_default();
                pending_route = Some((method.to_owned(), path));
            }
            continue;
        }
        if line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with('*')
            || line.starts_with("/*")
            || line.is_empty()
        {
            continue;
        }
        // Class/interface/object declarations: annotations attach here, and
        // a pending RequestMapping becomes the route prefix.
        let is_class_decl = line.contains(" class ")
            || line.starts_with("class ")
            || line.contains(" interface ")
            || line.starts_with("interface ")
            || line.contains(" object ")
            || line.starts_with("object ")
            || line.starts_with("trait ");
        if is_class_decl {
            if let Some(name) = declaration_name(line).or_else(|| class_name(line)) {
                for annotation in pending_annotations.drain(..) {
                    facts.annotations.push(AnnotationFact {
                        annotation,
                        target: name.clone(),
                    });
                }
                // A class-level RequestMapping immediately becomes the
                // prefix for subsequent method routes in the same file.
                if let Some((_, path)) = pending_route.take() {
                    class_prefix = Some(path);
                }
            }
            continue;
        }
        // Function/method declarations attach pending annotations + routes.
        if let Some(name) = declaration_name(line) {
            for annotation in pending_annotations.drain(..) {
                facts.annotations.push(AnnotationFact {
                    annotation,
                    target: name.clone(),
                });
            }
            if let Some((method, path)) = pending_route.take() {
                let path = match (&class_prefix, path.as_str()) {
                    (Some(prefix), p) if !prefix.is_empty() && !p.is_empty() => {
                        format!("{prefix}{p}")
                    }
                    (Some(prefix), "") => prefix.clone(),
                    _ => path,
                };
                facts.routes.push(RouteFact {
                    method,
                    path,
                    handler: name.clone(),
                });
            }
        } else {
            // A non-declaration, non-annotation line clears pending state.
            pending_annotations.clear();
            pending_route = None;
        }
    }
    facts
}

/// Identifier after `class`/`interface`/`object`/`trait` on a line.
fn class_name(line: &str) -> Option<String> {
    for kw in ["class ", "interface ", "object ", "trait "] {
        if let Some(pos) = line.find(kw) {
            let rest = &line[pos + kw.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {

    // ── JVM annotation layer ────────────────────────────────────────────

    #[test]
    fn java_spring_routes_compose_class_prefix_and_annotations() {
        let src = "@RestController\n@RequestMapping(\"/api/users\")\npublic class UserController {\n    @GetMapping(\"/list\")\n    public List<User> listUsers() { return repo.findAll(); }\n\n    @Override\n    @Transactional(readOnly = true)\n    public User find(Long id) { return repo.findById(id).orElse(null); }\n}\n";
        let facts = extract_jvm_facts(src);
        // Class-level prefix composes with method paths.
        assert!(
            facts.routes.iter().any(|r| r.method == "GET"
                && r.path == "/api/users/list"
                && r.handler == "listUsers"),
            "composed route: {:?}",
            facts.routes
        );
        // Annotations attach to both classes and methods.
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "RestController" && a.target == "UserController")
        );
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "GetMapping" && a.target == "listUsers")
        );
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "Transactional" && a.target == "find")
        );
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "Override" && a.target == "find")
        );
    }

    #[test]
    fn kotlin_annotations_attach() {
        let src = "class ApplicationTests {\n    @Test\n    fun contextLoads() { }\n}\n";
        let facts = extract_jvm_facts(src);
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "Test" && a.target == "contextLoads"),
            "kt annotations: {:?}",
            facts.annotations
        );
    }

    #[test]
    fn scala_annotations_attach() {
        let src = "@Service\nclass UserService {\n  @Transactional\n  def save(u: User): Unit = repo.save(u)\n}\n";
        let facts = extract_jvm_facts(src);
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "Service" && a.target == "UserService")
        );
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "Transactional" && a.target == "save")
        );
    }

    #[test]
    fn groovy_annotations_attach() {
        let src = "@Stepwise\nclass Spec extends Specification {\n    def \"a step\"() { }\n}\n";
        let facts = extract_jvm_facts(src);
        assert!(
            facts
                .annotations
                .iter()
                .any(|a| a.annotation == "Stepwise" && a.target == "Spec")
        );
    }

    use super::*;

    #[test]
    fn express_routes() {
        let facts = extract_frameworks(
            "const app = express();\napp.get('/users', listUsers);\napp.post('/users', createUser);\napp.use(logger);\n",
            "ts",
        );
        assert_eq!(facts.routes.len(), 2);
        assert!(facts.routes.contains(&RouteFact {
            method: "GET".into(),
            path: "/users".into(),
            handler: "listUsers".into()
        }));
        assert!(facts.routes.contains(&RouteFact {
            method: "POST".into(),
            path: "/users".into(),
            handler: "createUser".into()
        }));
    }

    #[test]
    fn nestjs_decorator_routes() {
        let facts = extract_frameworks(
            "@Controller('users')\nexport class UserController {\n  @Get(':id')\n  async find(@Param() p) {}\n}\n",
            "ts",
        );
        assert!(
            facts.routes.contains(&RouteFact {
                method: "GET".into(),
                path: ":id".into(),
                handler: "find".into()
            }),
            "{:?}",
            facts.routes
        );
    }

    #[test]
    fn fastapi_routes() {
        let facts = extract_frameworks("@app.get('/items')\ndef list_items():\n    pass\n", "py");
        assert!(
            facts.routes.contains(&RouteFact {
                method: "GET".into(),
                path: "/items".into(),
                handler: "list_items".into()
            }),
            "{:?}",
            facts.routes
        );
    }

    #[test]
    fn flask_routes() {
        let facts = extract_frameworks(
            "@app.route('/health', methods=['GET'])\ndef health():\n    return 'ok'\n",
            "py",
        );
        assert!(
            facts.routes.contains(&RouteFact {
                method: "GET".into(),
                path: "/health".into(),
                handler: "health".into()
            }),
            "{:?}",
            facts.routes
        );
    }

    #[test]
    fn spring_routes() {
        let facts = extract_frameworks(
            "@RestController\npublic class UserController {\n  @GetMapping(\"/users\")\n  public List<User> list() { }\n}\n",
            "java",
        );
        assert!(
            facts.routes.contains(&RouteFact {
                method: "GET".into(),
                path: "/users".into(),
                handler: "list".into()
            }),
            "{:?}",
            facts.routes
        );
    }

    #[test]
    fn react_callbacks_pair_component_with_handler() {
        let facts = extract_frameworks(
            "function UserList() {\n  return <button onClick={handleClick}>go</button>;\n}\n",
            "tsx",
        );
        assert!(
            facts
                .callbacks
                .contains(&("UserList".to_owned(), "handleClick".to_owned())),
            "{:?}",
            facts.callbacks
        );
    }

    #[test]
    fn stacked_decorators_do_not_steal_the_handler() {
        let facts = extract_frameworks(
            "@Get('/x')\n@UseInterceptors(LogInterceptor)\nasync find() {}\n",
            "ts",
        );
        assert!(
            facts.routes.contains(&RouteFact {
                method: "GET".into(),
                path: "/x".into(),
                handler: "find".into()
            }),
            "{:?}",
            facts.routes
        );
    }

    #[test]
    fn inline_arrow_handlers_are_rejected_not_misattributed() {
        let facts = extract_frameworks(
            "const app = {} as any;\napp.get('/x', (req, res) => res.send('hi'));\n",
            "ts",
        );
        assert!(
            facts.routes.is_empty(),
            "arrow must not yield a bogus handler: {:?}",
            facts.routes
        );
    }

    #[test]
    fn unsupported_extension_yields_empty() {
        assert!(extract_frameworks("class Foo", "rb").is_empty());
    }
}
