#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSvnId {
    pub url: String,
    pub revision: u32,
    pub uuid: String,
}

impl GitSvnId {
    pub fn parse(line: &str) -> Result<Self, String> {
        let rest = line
            .strip_prefix("git-svn-id: ")
            .ok_or_else(|| "missing git-svn-id prefix".to_string())?;
        let (url_rev, uuid) = rest
            .rsplit_once(' ')
            .ok_or_else(|| "missing uuid".to_string())?;
        let (url, rev) = url_rev
            .rsplit_once('@')
            .ok_or_else(|| "missing @revision".to_string())?;
        let revision = rev
            .parse::<u32>()
            .map_err(|_| format!("invalid revision: {rev}"))?;

        Ok(Self {
            url: url.to_string(),
            revision,
            uuid: uuid.to_string(),
        })
    }

    pub fn to_footer(&self) -> String {
        format!("git-svn-id: {}@{} {}", self.url, self.revision, self.uuid)
    }
}
