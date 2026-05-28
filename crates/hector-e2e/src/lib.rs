//! Docker-driven end-to-end harness for hector's shipping adapters.
//!
//! Public surface used by the tests in `tests/`:
//! - [`build_image`] / [`run_case`] — drive a per-adapter Docker container
//! - [`RunResult`] — captured forensics from one run
//! - [`require_e2e_env`] — preflight check (Docker, API key, hector binary)
//! - [`assertions`] — helpers each test composes against [`RunResult`]
//!
//! All tests in this crate are `#[ignore]` by default. Run with
//! `cargo test -p hector-e2e -- --ignored`.

pub mod assertions;
pub mod docker;
pub mod env;
pub mod result;

pub use docker::{build_image, run_case};
pub use env::require_e2e_env;
pub use result::RunResult;
