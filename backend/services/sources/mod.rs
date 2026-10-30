//! Where Safely reads sellers from, one folder per kind of source. Each
//! kind has its own standards file in backend/standards/ (what it reads
//! and checks). Inside a kind, sources that belong to one country go in
//! a folder for that country (b2b/china/alibaba.rs, later
//! databases/brazil/...); sources that work for every country stay at the
//! top of the kind's folder.
//!
//! - `b2b`: B2B marketplaces (Alibaba, TradeWheel, Global Sources...).
//! - `websites`: sellers' own shop websites.
//! - `social`: social media sellers (Facebook, WhatsApp, Instagram).
//! - `databases`: company databases (Crunchbase, company registers).
pub mod b2b;
pub mod databases;
pub mod social;
pub mod websites;
