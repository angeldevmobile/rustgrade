pub fn run() -> anyhow::Result<()> {
    let s = lib_api::load(&"app.toml".into())?;
    println!("{s}");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    run()
}
