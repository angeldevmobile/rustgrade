pub mod audit;
pub mod rules;
pub mod scan;

// TODO: install, uninstall, doctor, init.
pub fn not_yet(name: &str) -> anyhow::Result<u8> {
    anyhow::bail!("`{name}` is not implemented yet")
}
