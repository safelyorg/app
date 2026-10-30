//! Where Safely reads sellers from, one folder per kind of source. Each
//! kind has its own standards file in backend/standards/ (what it reads
//! and checks).
//!
//! - `b2b`: B2B marketplaces (Alibaba, TradeWheel, Global Sources...).
//! - `websites`: sellers' own shop websites.
//! - `social`: social media sellers (Facebook, WhatsApp, Instagram).
//! - `databases`: company databases (Crunchbase, company registers).
pub mod b2b;
pub mod databases;
pub mod social;
pub mod websites;
