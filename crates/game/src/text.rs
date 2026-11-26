//! Writing numbers and times for people.

/// A whole number with commas: 12345 -> "12,345".
pub fn thousands(n: f64) -> String {
    let n = n.round();
    let negative = n < 0.0;
    let digits = format!("{}", n.abs() as u64);
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if negative { format!("-{out}") } else { out }
}

/// Seconds as m:ss.mmm (like the 1.2.x game's formatTime).
pub fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0);
    let minutes = (s / 60.0).floor();
    let rest = s - minutes * 60.0;
    format!("{}:{:06.3}", minutes as u64, rest)
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers() {
        assert_eq!(super::thousands(1234567.0), "1,234,567");
        assert_eq!(super::thousands(12.0), "12");
        assert_eq!(super::format_time(83.456), "1:23.456");
    }
}
