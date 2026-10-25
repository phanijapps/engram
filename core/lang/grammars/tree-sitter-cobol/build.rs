fn main() {
    let src_dir = std::path::Path::new("src");
    let mut c_config = cc::Build::new();
    c_config.include(src_dir);
    c_config
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs");
    c_config.file(src_dir.join("parser.c"));
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("parser.c").display()
    );
    let scanner = src_dir.join("scanner.c");
    if scanner.exists() {
        c_config.file(&scanner);
        println!("cargo:rerun-if-changed={}", scanner.display());
    }
    c_config.compile("parser");
}
