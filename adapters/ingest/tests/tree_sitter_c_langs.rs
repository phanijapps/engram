use engram_ingest::TreeSitterChunker;

fn chunk(text: &str, ext: &str) -> Vec<(String, String)> {
    let chunker = TreeSitterChunker::new().expect("chunker");
    let candidates = chunker.chunk_with_ext(text, ext).expect("chunk");
    candidates
        .into_iter()
        .filter_map(|c| {
            let anchor = c.location?.anchor?;
            Some((anchor, c.text))
        })
        .collect()
}

#[test]
fn chunks_c_functions_and_structs() {
    let code = "int add(int a, int b) { return a + b; }\nstruct Point { int x; int y; };\n";
    let chunks = chunk(code, "c");
    assert!(
        chunks.iter().any(|(a, _)| a == "fn add"),
        "missing fn add: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _)| a == "struct Point"),
        "missing struct Point"
    );
}

#[test]
fn chunks_cpp_classes_and_methods() {
    let code = "class Engine {\n  void start() {}\n  void stop() {}\n};\nnamespace Core { int init() { return 0; } }\n";
    let chunks = chunk(code, "cpp");
    assert!(
        chunks.iter().any(|(a, _)| a == "class Engine"),
        "missing class Engine: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _)| a == "fn start"),
        "missing fn start"
    );
    assert!(
        chunks.iter().any(|(a, _)| a == "fn stop"),
        "missing fn stop"
    );
    assert!(
        chunks.iter().any(|(a, _)| a == "fn init"),
        "missing fn init (namespace)"
    );
}

#[test]
fn chunks_csharp_classes_and_methods() {
    let code = "public class Program {\n  static void Main() {}\n  void Run() {}\n}\npublic interface IPlugin {}\n";
    let chunks = chunk(code, "cs");
    assert!(
        chunks.iter().any(|(a, _)| a == "class Program"),
        "missing class Program: {chunks:?}"
    );
    assert!(
        chunks.iter().any(|(a, _)| a == "fn Main"),
        "missing fn Main"
    );
    assert!(chunks.iter().any(|(a, _)| a == "fn Run"), "missing fn Run");
    assert!(
        chunks.iter().any(|(a, _)| a == "interface IPlugin"),
        "missing interface IPlugin"
    );
}

#[test]
fn chunks_header_files_as_c() {
    let code = "#ifndef FOO_H\n#define FOO_H\nstruct Config { int debug; };\n#endif\n";
    let chunks = chunk(code, "h");
    assert!(
        chunks.iter().any(|(a, _)| a == "struct Config"),
        "missing struct Config: {chunks:?}"
    );
}
