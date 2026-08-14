use fancy_regex::Regex;

#[cfg(test)]
thread_local! {
    static REGEX_COMPILATION_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterDecision {
    Include,
    Exclude,
}

#[derive(Debug, Clone)]
pub struct PathFilters {
    include: Option<Regex>,
    ignore: Option<Regex>,
}

impl PathFilters {
    pub fn new(include: Option<String>, ignore: Option<String>) -> Result<Self, String> {
        Ok(Self {
            include: compile_regex(include)?,
            ignore: compile_regex(ignore)?,
        })
    }

    pub fn decide(&self, path: &str) -> Result<FilterDecision, String> {
        if path.split('/').any(|part| part == ".git") {
            return Ok(FilterDecision::Exclude);
        }

        if matches_regex(&self.ignore, path)? {
            return Ok(FilterDecision::Exclude);
        }

        if let Some(include) = &self.include {
            return if include.is_match(path).map_err(|e| e.to_string())? {
                Ok(FilterDecision::Include)
            } else {
                Ok(FilterDecision::Exclude)
            };
        }

        Ok(FilterDecision::Include)
    }
}

fn compile_regex(pattern: Option<String>) -> Result<Option<Regex>, String> {
    match pattern {
        Some(pattern) => {
            #[cfg(test)]
            REGEX_COMPILATION_COUNT.with(|count| count.set(count.get() + 1));
            Regex::new(&pattern)
                .map(Some)
                .map_err(|err| err.to_string())
        }
        None => Ok(None),
    }
}

#[cfg(test)]
pub(crate) fn reset_regex_compilation_count() {
    REGEX_COMPILATION_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn regex_compilation_count() -> usize {
    REGEX_COMPILATION_COUNT.with(std::cell::Cell::get)
}

fn matches_regex(regex: &Option<Regex>, path: &str) -> Result<bool, String> {
    match regex {
        Some(regex) => regex.is_match(path).map_err(|err| err.to_string()),
        None => Ok(false),
    }
}
