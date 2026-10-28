use rustgrade_core::rules::{self, Rule};
use serde_json::json;

use crate::{EXIT_OK, Format, Globals};

pub fn run(explain: Option<&str>, globals: &Globals) -> anyhow::Result<u8> {
    let json = match globals.format {
        Format::Json => true,
        Format::Text => false,
        Format::Sarif | Format::Markdown => {
            anyhow::bail!("`rules` supports --format text or json")
        }
    };

    if let Some(id) = explain {
        let rule = rules::find(id).ok_or_else(|| {
            anyhow::anyhow!("unknown rule `{id}`; run `rustgrade rules` to list them")
        })?;
        if json {
            let mut value = describe(rule);
            value["explain"] = json!(rule.explain());
            println!("{}", serde_json::to_string_pretty(&value)?);
        } else {
            println!(
                "{} ({}, {})\n{}\n\n{}",
                rule.id(),
                rule.category().as_str(),
                rule.default_severity(),
                rule.summary(),
                rule.explain().trim_end()
            );
        }
        return Ok(EXIT_OK);
    }

    if json {
        let list: Vec<_> = rules::registry().iter().map(|r| describe(*r)).collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
    } else {
        for rule in rules::registry() {
            println!(
                "{:<22} {:<9} {:<12} {}",
                rule.id(),
                rule.default_severity(),
                rule.category().as_str(),
                rule.summary()
            );
        }
    }
    Ok(EXIT_OK)
}

fn describe(rule: &dyn Rule) -> serde_json::Value {
    json!({
        "id": rule.id(),
        "category": rule.category().as_str(),
        "severity": rule.default_severity().as_str(),
        "summary": rule.summary(),
    })
}
