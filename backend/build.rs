// Rebuild the backend whenever a migration file is added or changed.
// sqlx::migrate!("../migrations") in main.rs bundles the migration files
// into the program when it is built, but cargo only notices changes to
// Rust files by itself. Without this, a new .sql file is left out and the
// backend stops at startup with "VersionMissing".
fn main() {
    println!("cargo:rerun-if-changed=../migrations");
}
