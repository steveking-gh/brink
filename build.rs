// This code validates that CARGO_PKG_VERSION contains only digit characters in the
// major/minor/patch fields.  You get a warning at build time if you set
// a pre-release suffix like 4.0.0-beta that would break the version builtins
// (__BRINK_VERSION_MAJOR etc.).
fn main() {
    let version = env!("CARGO_PKG_VERSION");

    // Accepts semver-like version strings; warn if major/minor/patch includes non-digit characters.
    let parts: Vec<_> = version.split('.').collect();
    for (idx, part) in parts.iter().enumerate().take(3) {
        if part.chars().any(|c| !c.is_ascii_digit()) {
            let field = match idx {
                0 => "major",
                1 => "minor",
                2 => "patch",
                _ => "unknown",
            };
            println!(
                "cargo:warning=CARGO_PKG_VERSION '{}' has non-digit {} field ('{}').",
                version, field, part
            );
        }
    }
}
