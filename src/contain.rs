//! Panics in other crates' parsers, caught so that one document can't end
//! a run. A scanner calls a parser through `catch`, and falls back on a
//! panic as it does on a parse error.
//!
//! The panic hook runs before a panic unwinds, and the default one prints
//! it. [`quiet_hook`] keeps it quiet for the panics `catch` catches; the
//! binary and the fuzz targets install it.

use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};

thread_local! {
    /// Whether this thread is inside [`catch`].
    static CATCHING: Cell<bool> = const { Cell::new(false) };
}

/// What `f` returns, or `None` when it panics. Only for calls that hold no
/// state of ours, which a panic could leave half changed.
pub(crate) fn catch<T>(f: impl FnOnce() -> T) -> Option<T> {
    let outer = CATCHING.replace(true);
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    CATCHING.set(outer);
    result.ok()
}

/// Wraps the panic hook so that it passes over the panics `catch`
/// catches and handles every other one as before. Where panics abort,
/// nothing is caught, so the hook passes over none.
pub fn quiet_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if cfg!(panic = "abort") || !CATCHING.get() {
            previous(info);
        }
    }));
}
