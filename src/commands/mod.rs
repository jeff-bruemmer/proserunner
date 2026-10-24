//! The commands `main` runs, and what they share: where output goes, and
//! the settings a command loads.

mod check;
mod config;
mod console;
mod ignore;
mod init;
mod migrate;
mod rules;
mod setup;

pub(crate) use check::check;
pub(crate) use config::config_command;
pub(crate) use ignore::ignore_command;
pub(crate) use init::init;
pub(crate) use migrate::migrate_command;
pub(crate) use rules::{explain_command, rules_command};

pub(crate) const EXIT_OK: u8 = 0;
pub(crate) const EXIT_ISSUES: u8 = 1;
pub(crate) const EXIT_ERROR: u8 = 2;
