//! Fixture: a request handler that panics instead of returning errors.

use std::collections::HashMap;

fn handle(req: &str, users: &HashMap<String, u32>) -> String {
    let (name, age) = req.split_once(':').unwrap();
    let age: u32 = age.trim().parse().expect("age is a number");
    let known = users.get(name).copied().unwrap_or(0);
    format!("{name} {age} {known}")
}

fn main() {
    let users = HashMap::new();
    println!("{}", handle("ana:30", &users));
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses() {
        assert_eq!("1".parse::<u32>().unwrap(), 1);
    }
}
