// Rebuild when a migration is added, so `sqlx::migrate!` embeds it
// (generated shape of `sqlx migrate build-script`).
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
