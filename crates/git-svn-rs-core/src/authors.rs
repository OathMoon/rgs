use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorResolver {
    by_login: BTreeMap<String, Author>,
}

impl AuthorResolver {
    pub fn resolve(&self, login: &str) -> Option<&Author> {
        self.by_login.get(login)
    }
}

pub fn parse_authors_file(input: &str) -> Result<AuthorResolver, String> {
    let mut resolver = AuthorResolver::default();
    for (idx, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (login, rest) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid authors line {}: missing '='", idx + 1))?;
        let rest = rest.trim();
        let start = rest
            .rfind('<')
            .ok_or_else(|| format!("invalid authors line {}: missing '<'", idx + 1))?;
        let end = rest
            .rfind('>')
            .ok_or_else(|| format!("invalid authors line {}: missing '>'", idx + 1))?;
        if end < start {
            return Err(format!("invalid authors line {}: malformed email", idx + 1));
        }

        resolver.by_login.insert(
            login.trim().to_string(),
            Author {
                name: rest[..start].trim().to_string(),
                email: rest[start + 1..end].trim().to_string(),
            },
        );
    }
    Ok(resolver)
}
