use rustgrade_core::rules;

use crate::{EXIT_OK, Globals};

// TODO: `--explain` should print rationale and before/after code, and honor `--format`.
pub fn run(explain: Option<&str>, _globals: &Globals) -> anyhow::Result<u8> {
    if let Some(id) = explain {
        let rule = rules::find(id).ok_or_else(|| anyhow::anyhow!("unknown rule `{id}`"))?;
        println!("{}  [{:?}, {:?}]", rule.id(), rule.category(), rule.default_severity());
        println!("{}", rule.summary());
        return Ok(EXIT_OK);
    }
    for rule in rules::registry() {
        let severity = format!("{:?}", rule.default_severity());
        println!("{:<22} {severity:<9} {}", rule.id(), rule.summary());
    }
    Ok(EXIT_OK)
}
