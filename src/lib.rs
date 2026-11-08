pub mod ai;
pub mod cards;
pub mod eval;
pub mod client;
pub mod game;
pub mod proto;
pub mod server;

/// Форматирование сумм: «12 500», «1.25M»
pub fn fmt_chips(n: u64) -> String {
    if n >= 10_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1e6)
    } else {
        let s = n.to_string();
        let mut out = String::new();
        for (i, ch) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push('\u{2009}');
            }
            out.push(ch);
        }
        out
    }
}

