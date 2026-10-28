use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=locales");
    let mut files: Vec<_> = fs::read_dir("locales")
        .expect("locales directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    files.sort();

    let mut generated = String::from("pub const EMBEDDED_LOCALES: &[(&str, &str)] = &[\n");
    for path in files {
        let code = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("locale filename");
        let absolute = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        generated.push_str(&format!(
            "    ({code:?}, include_str!(r#\"{}\"#)),\n",
            absolute.display()
        ));
        let _ = Path::new(&path);
    }
    generated.push_str("];\n");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    fs::write(Path::new(&out).join("locales.rs"), generated).expect("write generated locales");
}
