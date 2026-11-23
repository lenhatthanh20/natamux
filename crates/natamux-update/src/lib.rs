//! Signed release checks for the native app. Network and install steps call these rules.

pub mod check;
pub mod download;
pub mod install;
pub mod offline;
pub mod policy;
pub mod preferences;
pub mod signature;
pub mod tarball;
pub mod transport;
pub mod updater;

#[cfg(test)]
mod check_tests;
#[cfg(test)]
mod download_tests;
#[cfg(test)]
mod install_tests;
#[cfg(test)]
mod preferences_tests;
#[cfg(test)]
mod signature_tests;
#[cfg(test)]
mod tarball_tests;
#[cfg(test)]
mod transport_tests;
#[cfg(test)]
mod updater_tests;
