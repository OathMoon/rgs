use fancy_regex::Regex;

#[derive(Debug, Clone)]
pub struct GlobSpec {
    left: String,
    right: String,
    depth: usize,
    regex: Regex,
    glob: String,
}

impl GlobSpec {
    pub fn new(glob: &str, pattern_ok: bool) -> Result<Self, String> {
        let glob = glob.trim_matches('/').to_string();
        let die_msg =
            format!("Only one set of wildcards (e.g. '*' or '*/*/*') is supported: {glob}");
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut patterns = Vec::new();
        let mut saw_pattern = false;
        let mut saw_right = false;

        for part in glob.split('/') {
            if part.is_empty() {
                continue;
            }

            let stars = part.matches('*').count();
            if stars > 1 {
                return Err(format!("Only one '*' is allowed in a pattern: '{part}'"));
            }

            let pattern = if let Some(pos) = part.find('*') {
                Some(format!(
                    "{}[^/]*{}",
                    escape_regex(&part[..pos]),
                    escape_regex(&part[pos + 1..])
                ))
            } else if pattern_ok && part.starts_with('{') && part.ends_with('}') {
                let inner = &part[1..part.len() - 1];
                let alternatives = inner
                    .split(',')
                    .map(escape_regex)
                    .collect::<Vec<_>>()
                    .join("|");
                Some(format!("(?:{alternatives})"))
            } else {
                if pattern_ok && (part.contains('{') || part.contains('}')) {
                    return Err(format!("Invalid pattern in '{glob}': {part}"));
                }
                None
            };

            if let Some(pattern) = pattern {
                if saw_right {
                    return Err(die_msg);
                }
                saw_pattern = true;
                patterns.push(pattern);
            } else if saw_pattern {
                saw_right = true;
                right.push(part.to_string());
            } else {
                left.push(part.to_string());
            }
        }

        if patterns.is_empty() {
            return Err(format!("One '*' is needed in glob: '{glob}'"));
        }

        let left = left.join("/");
        let right = right.join("/");
        let middle = patterns.join("/");
        let mut pieces = Vec::new();
        if !left.is_empty() {
            pieces.push(escape_regex(&left));
        }
        pieces.push(middle);
        if !right.is_empty() {
            pieces.push(escape_regex(&right));
        }
        let regex = Regex::new(&format!("^{}$", pieces.join("/"))).map_err(|e| e.to_string())?;

        Ok(Self {
            left,
            right,
            depth: patterns.len(),
            regex,
            glob,
        })
    }

    pub fn left(&self) -> &str {
        &self.left
    }

    pub fn right(&self) -> &str {
        &self.right
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn glob(&self) -> &str {
        &self.glob
    }

    pub fn full_path(&self, path: &str) -> String {
        match (self.left.is_empty(), self.right.is_empty()) {
            (true, true) => path.to_string(),
            (false, true) => format!("{}/{}", self.left, path),
            (true, false) => format!("{}/{}", path, self.right),
            (false, false) => format!("{}/{}/{}", self.left, path, self.right),
        }
    }

    pub fn is_match(&self, path: &str) -> bool {
        self.regex.is_match(path).unwrap_or(false)
    }
}

fn escape_regex(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
            | '#' | '&' | '-' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            other => escaped.push(other),
        }
    }
    escaped
}
