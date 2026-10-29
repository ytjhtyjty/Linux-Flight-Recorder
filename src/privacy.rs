use regex::Regex;
use std::sync::OnceLock;
pub fn visible(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_control()
            || matches ! (c , '\u{202a}' ..='\u{202e}' | '\u{2066}' ..='\u{2069}' | '\u{200b}' | '\u{200e}' | '\u{200f}' | '<' | '>' | '[' | ']' | '`' | '*' | '_' | '#' | '|' | '\\' | '!')
        {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}
fn patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| {
        [
            r"(?i)\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b",
            r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b",
            r"/(?:home|Users)/[^/\s]+(?:/[^\s]*)?",
            r"/root(?:/[^\s]*)?",
            r"(?i)(?:bearer\s+|(?:token|password|passwd|secret|api[_-]?key)\s*[:=]\s*)[^\s,;]+",
            r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("static regex"))
        .collect()
    })
}
pub fn redact(s: &str, known: &[String]) -> String {
    let mut s = s.to_string();
    for k in known.iter().filter(|k| k.len() >= 3) {
        s = s.replace(k, "[identity]");
    }
    static IPV6: OnceLock<Regex> = OnceLock::new();
    let ipv6 = IPV6.get_or_init(|| {
        Regex::new(r"(?i)[0-9a-f:.]+(?:%[0-9a-z_-]+)?").expect("static IPv6 candidates")
    });
    s = ipv6
        .replace_all(&s, |c: &regex::Captures<'_>| {
            let candidate = c[0].trim_end_matches('.');
            if candidate
                .split('%')
                .next()
                .unwrap_or("")
                .parse::<std::net::Ipv6Addr>()
                .is_ok()
            {
                "[redacted]".to_owned()
            } else {
                c[0].to_owned()
            }
        })
        .into_owned();
    for p in patterns() {
        s = p.replace_all(&s, "[redacted]").into_owned();
    }
    s
}
pub fn public_snapshot(s: &crate::model::Snapshot) -> crate::model::Snapshot {
    let mut s = s.clone();
    s.boot_id = None;
    s.label = redact(&s.label, &[]);
    for set in s.collectors.values_mut() {
        for r in &mut set.records {
            r.remove("cursor");
            r.remove("boot_id");
            for (k, v) in r.iter_mut() {
                if ["message", "id", "kernel_version"].contains(&k.as_str()) {
                    *v = redact(v, &[]);
                }
            }
        }
        for l in &mut set.limitations {
            *l = redact(l, &[]);
        }
    }
    s
}
