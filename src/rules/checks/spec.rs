//! What each check declares about itself.

/// What a check declares about itself.
pub(crate) struct Spec {
    /// Its key in a rule file.
    pub key: &'static str,
    /// The placeholders its messages can use besides the common ones, in
    /// the order its issues' `args` hold their values.
    pub vars: &'static [&'static str],
    /// Its message when the rule has none.
    pub message: &'static str,
    /// What its issues suggest, written with its placeholders. A check
    /// without suggestions can't use `{suggestion}`.
    pub suggest: &'static [&'static str],
}

/// How `explain` shows a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Description {
    /// The check as a rule file writes it.
    pub written: String,
    /// What it finds, on one line or more.
    pub finds: String,
}
