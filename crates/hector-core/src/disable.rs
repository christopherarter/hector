use std::collections::BTreeMap;

/// Maps line number → set of rule_ids disabled on that line.
#[derive(Debug, Default)]
pub struct DisableMap {
    by_line: BTreeMap<u32, Vec<String>>,
}

impl DisableMap {
    pub fn from_source(src: &str) -> Self {
        let mut map = Self::default();
        for (i, line) in src.lines().enumerate() {
            let line_no = (i as u32) + 1;
            for rule_id in parse_disable_directives(line) {
                map.by_line.entry(line_no).or_default().push(rule_id);
            }
        }
        map
    }

    pub fn is_disabled(&self, line: u32, rule_id: &str) -> bool {
        self.by_line
            .get(&line)
            .map(|rules| rules.iter().any(|r| r == rule_id))
            .unwrap_or(false)
    }
}

fn parse_disable_directives(line: &str) -> Vec<String> {
    let marker = "hector-disable:";
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(idx) = rest.find(marker) {
        let after = &rest[idx + marker.len()..];
        let trimmed = after.trim_start();
        let end = trimmed
            .find(|c: char| c.is_whitespace() || c == '*' || c == '/')
            .unwrap_or(trimmed.len());
        let rule_id = &trimmed[..end];
        if !rule_id.is_empty() {
            out.push(rule_id.to_string());
        }
        rest = &after[end..];
    }
    out
}
