use super::{RevisionEvent, SvnBackend};

#[derive(Debug, Default)]
pub struct LibSvnBackend;

impl LibSvnBackend {
    pub fn new() -> Self {
        Self
    }
}

impl SvnBackend for LibSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        Err("libsvn backend is not implemented yet".to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        Err("libsvn backend is not implemented yet".to_string())
    }

    fn log(&self, _start: u32, _end: u32) -> Result<Vec<RevisionEvent>, String> {
        Err("libsvn backend is not implemented yet".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_type_is_constructible_without_ffi() {
        let backend = LibSvnBackend::new();

        assert_eq!(
            backend.uuid().unwrap_err(),
            "libsvn backend is not implemented yet"
        );
    }
}
