/// Whether the crate was found as itself or under an alias.
#[derive(Debug, Clone)]
pub enum FoundCrate {
    Itself,
    Name(String),
}

#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

/// Stub implementation: resolves crate name from CARGO_PKG_NAME or returns
/// the canonical name. Always returns Ok so proc-macro callers don't panic.
pub fn crate_name(orig_name: &str) -> Result<FoundCrate, Error> {
    let pkg = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    let normalised = |s: &str| s.replace('-', "_");
    if normalised(&pkg) == normalised(orig_name) {
        Ok(FoundCrate::Itself)
    } else {
        // Return the canonical (underscore) name so proc-macro callers generate
        // correct `::crate_name::` paths without panicking.
        Ok(FoundCrate::Name(normalised(orig_name)))
    }
}
