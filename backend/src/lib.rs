// The reason for moving the modules path here is to make the Rust aware
// that backend package exists that can be accessed by tests.
#[path = "../db/mod.rs"]
pub mod db;
#[path = "../errors/mod.rs"]
pub mod errors;
#[path = "../handlers/mod.rs"]
pub mod handlers;
#[path = "../models/mod.rs"]
pub mod models;
#[path = "../routes/mod.rs"]
pub mod routes;
#[path = "../services/mod.rs"]
pub mod services;
