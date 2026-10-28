pub fn parse(src: &str) -> anyhow::Result<u32> {
    Ok(src.trim().parse()?)
}
