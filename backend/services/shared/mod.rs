//! Helpers used by the features and sources, never called by a handler
//! on its own (Claude, WHOIS, ScraperAPI, evidence, confidence...).
//! platform_config is used by a route and scraper_client by main.rs, not
//! by a handler, so they are here too.
pub mod claude;
pub mod confidence;
pub mod evidence;
pub mod platform_config;
pub mod scraper_client;
pub mod standards;
pub mod whois;
