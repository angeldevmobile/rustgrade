//! Fixture: tutorial-style Rust that clones and allocates per iteration,
//! plus raw-pointer code with no safety argument.

pub struct Order {
    pub customer: String,
    pub items: Vec<String>,
}

pub fn summarize(orders: &[Order]) -> Vec<String> {
    let mut out = Vec::new();
    for order in orders {
        let customer = order.customer.clone();
        let mut line = String::new();
        for item in &order.items {
            line.push_str(&format!("{item}, "));
        }
        out.push(format!("{customer}: {line}"));
    }
    out
}

pub fn first_byte(v: &[u8]) -> u8 {
    unsafe { *v.as_ptr() }
}

pub fn first_byte_checked(v: &[u8]) -> Option<u8> {
    if v.is_empty() {
        return None;
    }
    // SAFETY: `v` is non-empty, so its pointer is valid for one read.
    Some(unsafe { *v.as_ptr() })
}
