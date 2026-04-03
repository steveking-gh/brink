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
