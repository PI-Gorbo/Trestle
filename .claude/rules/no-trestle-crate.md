# Do not modify the `trestle` crate (except its tests)

Never edit, create, or delete files under `crates/trestle/` (source or grammar).
The user writes the compiler themselves. You may read it, build it, and run its tests,
then report what is wrong — but leave the fix to the user unless they explicitly
ask you to change a specific file in this crate in the current request.

Tests are the exception: you may add, edit, and delete test programs and snapshots under
`crates/trestle/tests/`, and the corpus entries in `crates/trestle/tests/corpus.rs`.
Unit tests inside `crates/trestle/src/` (`#[cfg(test)]` modules) are still off-limits,
because they live in compiler source files.
