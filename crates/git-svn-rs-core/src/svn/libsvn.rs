use super::{RevisionEvent, SvnBackend};

pub const LIBSVN_NOT_LINKED_MESSAGE: &str =
    "libsvn backend is enabled but not linked: no libsvn FFI bindings are compiled into this build";

#[derive(Debug, Default)]
pub struct LibSvnBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibSvnLinkStatus {
    NotLinked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibSvnAvailability {
    pub feature_enabled: bool,
    pub link_status: LibSvnLinkStatus,
    pub version: Option<&'static str>,
    pub detail: &'static str,
}

impl LibSvnBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn availability() -> LibSvnAvailability {
        LibSvnAvailability {
            feature_enabled: true,
            link_status: LibSvnLinkStatus::NotLinked,
            version: None,
            detail: LIBSVN_NOT_LINKED_MESSAGE,
        }
    }

    pub fn version(&self) -> Result<String, String> {
        Err(LIBSVN_NOT_LINKED_MESSAGE.to_string())
    }
}

impl SvnBackend for LibSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        Err(LIBSVN_NOT_LINKED_MESSAGE.to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        Err(LIBSVN_NOT_LINKED_MESSAGE.to_string())
    }

    fn log(&self, _start: u32, _end: u32) -> Result<Vec<RevisionEvent>, String> {
        Err(LIBSVN_NOT_LINKED_MESSAGE.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_type_is_constructible_without_ffi() {
        let backend = LibSvnBackend::new();

        assert_eq!(backend.uuid().unwrap_err(), LIBSVN_NOT_LINKED_MESSAGE);
    }
}
