//! Probe: what happens to annotations in Java/Kotlin/Scala today?
#[test]
fn annotation_behavior() {
    let ts = engram_code::TreeSitterChunker::new().unwrap();

    let java = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {
    @GetMapping("/list")
    public List<User> listUsers() { return repo.findAll(); }

    @Override
    @Transactional(readOnly = true)
    public User find(Long id) { return repo.findById(id).orElse(null); }
}
"#;
    let kotlin = r#"
@Composable
fun MyScreen(state: UiState) {
    Text(text = state.title)
}

@SpringBootTest
class ApplicationTests {
    @Test
    fun contextLoads() { }
}
"#;
    let scala = r#"
@main def hello(): Unit = println("hi")

@Service
class UserService {
  @Transactional
  def save(u: User): Unit = repo.save(u)
}
"#;

    for (ext, src, name) in [
        ("java", java, "JAVA"),
        ("kt", kotlin, "KOTLIN"),
        ("scala", scala, "SCALA"),
    ] {
        let chunks = ts.chunk_with_ext(src, ext).unwrap_or_default();
        eprintln!("== {name} ==");
        for ch in &chunks {
            let anchor = ch
                .location
                .as_ref()
                .and_then(|l| l.anchor.clone())
                .unwrap_or_default();
            let first_lines: String = ch.text.lines().take(3).collect::<Vec<_>>().join(" ⏎ ");
            eprintln!("  anchor={anchor:?} text_head={first_lines:?}");
        }
        // Does the lexical text include the annotation? Search "@Transactional" style.
        let has_annot_text = chunks.iter().any(|c| c.text.contains('@'));
        eprintln!("  annotation text in chunk bodies: {has_annot_text}");
        // Do the annotations produce anchors/entities of their own?
        let annot_anchors = chunks
            .iter()
            .filter(|c| {
                c.location
                    .as_ref()
                    .and_then(|l| l.anchor.clone())
                    .map(|a| a.contains('@'))
                    .unwrap_or(false)
            })
            .count();
        eprintln!("  annotation anchors: {annot_anchors}");
    }
}
