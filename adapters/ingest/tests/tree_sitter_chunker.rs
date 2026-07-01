use engram_ingest::TreeSitterChunker;

fn chunk(text: &str, ext: &str) -> Vec<(String, String, u32, u32)> {
    let chunker = TreeSitterChunker::new().expect("chunker");
    let candidates = chunker.chunk_with_ext(text, ext).expect("chunk");
    candidates
        .into_iter()
        .filter_map(|c| {
            let loc = c.location?;
            let anchor = loc.anchor?;
            Some((
                anchor,
                c.text,
                loc.start_line.unwrap_or(0),
                loc.end_line.unwrap_or(0),
            ))
        })
        .collect()
}

#[test]
fn chunks_rust_functions_and_structs() {
    let code = "fn alpha() { beta(); }\nfn beta() {}\nstruct Widget;\n";
    let chunks = chunk(code, "rs");
    assert_eq!(chunks.len(), 3, "expected 3 declarations: {chunks:?}");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "fn alpha"),
        "missing fn alpha"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "fn beta"),
        "missing fn beta"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "struct Widget"),
        "missing struct Widget"
    );
}

#[test]
fn chunks_typescript_functions_and_classes() {
    let code = "function greet(name: string) {}\nclass Greeter {}\ninterface IGreet {}\n";
    let chunks = chunk(code, "ts");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "function greet"),
        "missing greet: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "class Greeter"),
        "missing Greeter"
    );
}

#[test]
fn chunks_python_defs_and_classes() {
    let code = "def alpha():\n    pass\n\nclass Widget:\n    pass\n";
    let chunks = chunk(code, "py");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "def alpha"),
        "missing def alpha: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "class Widget"),
        "missing class Widget"
    );
}

#[test]
fn chunks_java_methods_and_classes() {
    let code = "class Main {\n  void run() {}\n  String getName() { return \"\"; }\n}\n";
    let chunks = chunk(code, "java");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "class Main"),
        "missing class Main: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "fn run"),
        "missing fn run"
    );
}

#[test]
fn chunks_bash_functions() {
    let code = "greet() { echo hi; }\nfunction bye { echo bye; }\n";
    let chunks = chunk(code, "sh");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a.starts_with("fn")),
        "expected at least 1 function: {chunks:?}"
    );
}

#[test]
fn chunks_php_functions_and_classes() {
    let code = "<?php\nfunction greet($name) {}\nclass Greeter {}\n?>\n";
    let chunks = chunk(code, "php");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "fn greet"),
        "missing fn greet: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "class Greeter"),
        "missing class Greeter"
    );
}

#[test]
fn chunks_kotlin_functions_and_classes() {
    let code = "fun greet(name: String) {}\nclass Greeter {}\nobject Singleton {}\n";
    let chunks = chunk(code, "kt");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "fn greet"),
        "missing fn greet: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "class Greeter"),
        "missing class Greeter"
    );
}

#[test]
fn chunks_apex_methods_and_classes() {
    let code = "public class Main {\n  void run() {}\n  String getName() { return ''; }\n}\n";
    let chunks = chunk(code, "cls");
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "class Main"),
        "missing class Main: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _, _, _)| a == "fn run"),
        "missing fn run"
    );
}

#[test]
fn unsupported_extension_returns_error() {
    let chunker = TreeSitterChunker::new().expect("chunker");
    let result = chunker.chunk_with_ext("fn x() {}", "vim");
    assert!(result.is_err(), "expected error for unsupported extension");
}
