//! Smoke tests for the crate's public surface.
use tessera::VERSION;

#[test]
fn version_matches_manifest() {
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}
