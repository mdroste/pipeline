// Compile the actual production address predicate without pulling in Tauri or
// the unrelated public-web HTTP client. Fail if the source boundaries change.
fn main() {
    let source = "../../gui/src-tauri/src/public_web.rs";
    println!("cargo:rerun-if-changed={source}");
    let text = std::fs::read_to_string(source).unwrap();
    let start = text.find("pub(crate) fn public_ip(").unwrap();
    let end = text[start..].find("\npub(crate) fn url(").unwrap() + start;
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("public_ip.rs"), &text[start..end]).unwrap();
}
