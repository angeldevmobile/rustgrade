pub mod audit;
pub mod doctor;
pub mod install;
pub mod rules;
pub mod scan;

// TODO: init.
pub fn not_yet(name: &str) -> anyhow::Result<u8> {
    anyhow::bail!("`{name}` is not implemented yet")
}
