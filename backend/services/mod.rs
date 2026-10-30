//! Safely's services, in three groups:
//!
//! - `features`: what the handlers call directly - one file per thing a
//!   user can do (scan, sign in, pay, report, see history...).
//! - `shared`: helpers the features use but no handler calls on its own
//!   (Claude, WHOIS, ScraperAPI, evidence, confidence...).
//! - `sources`: where Safely reads sellers from, one folder per kind of
//!   source (B2B marketplaces, sellers' own websites, social media,
//!   company databases).
//!
//! To add a file: if a handler calls it, put it in `features`; if only
//! other services use it, put it in `shared`; if it reads a kind of
//! seller page, put it in the matching `sources` folder.
pub mod features;
pub mod shared;
pub mod sources;
