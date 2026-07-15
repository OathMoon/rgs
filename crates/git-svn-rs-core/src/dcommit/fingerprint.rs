use std::path::Path;

use sha2::{Digest, Sha256};

use super::diff_planner::{
    ChangeMetadata, CopySource, DcommitPlan, DcommitTarget, PlannedChange, PlannedChangeKind,
    PropertyChange,
};
use super::journal::DcommitTargetIdentity;

const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct RecoveryFingerprintInput<'a> {
    pub target: &'a DcommitTargetIdentity,
    pub no_rebase: bool,
    pub mergeinfo: Option<&'a str>,
}

pub fn canonical_plan_bytes(plan: &DcommitPlan) -> Vec<u8> {
    let mut encoder = Encoder::new("git-svn-rs/dcommit-plan");
    encode_plan(&mut encoder, plan);
    encoder.finish()
}

pub fn plan_fingerprint(plan: &DcommitPlan) -> String {
    fingerprint(&canonical_plan_bytes(plan))
}

pub fn canonical_message_bytes(message: &str) -> Vec<u8> {
    let mut encoder = Encoder::new("git-svn-rs/dcommit-message");
    encoder.field_str("message", message);
    encoder.finish()
}

pub fn message_fingerprint(message: &str) -> String {
    fingerprint(&canonical_message_bytes(message))
}

pub fn canonical_recovery_config_bytes(input: RecoveryFingerprintInput<'_>) -> Vec<u8> {
    let mut encoder = Encoder::new("git-svn-rs/dcommit-recovery-config");
    encoder.structure("target", |encoder| {
        encoder.field_str("remote_id", &input.target.remote_id);
        encoder.field_str("repository_root_url", &input.target.repository_root_url);
        encoder.field_str("repository_uuid", &input.target.repository_uuid);
        encoder.field_str("mapping_ref", &input.target.mapping_ref);
        encoder.field_path("rev_map_path", Path::new(&input.target.rev_map_path));
        encoder.field_str("commit_url", &input.target.commit_url);
    });
    encoder.field_bool("no_rebase", input.no_rebase);
    encoder.option("mergeinfo", input.mergeinfo, |encoder, value| {
        encoder.value_str(value);
    });
    encoder.finish()
}

pub fn recovery_config_fingerprint(input: RecoveryFingerprintInput<'_>) -> String {
    fingerprint(&canonical_recovery_config_bytes(input))
}

fn fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn encode_plan(encoder: &mut Encoder, plan: &DcommitPlan) {
    encode_target(encoder, &plan.target);
    encoder.field_u32("base_revision", plan.base_revision);
    encoder.field_str("git_commit", &plan.git_commit);
    encoder.field_str("message", &plan.message);
    encoder.option("author", plan.author.as_deref(), |encoder, value| {
        encoder.value_str(value);
    });
    encoder.sequence("root_properties", &plan.root_properties, encode_property);
    encoder.sequence("changes", &plan.changes, encode_change);
}

fn encode_target(encoder: &mut Encoder, target: &DcommitTarget) {
    encoder.structure("target", |encoder| {
        encoder.field_str("url", &target.url);
        encoder.field_str("repository_root", &target.repository_root);
        encoder.field_str("repository_uuid", &target.repository_uuid);
        encoder.field_str("git_ref", &target.git_ref);
    });
}

fn encode_property(encoder: &mut Encoder, property: &PropertyChange) {
    encoder.value_str(&property.name);
    encoder.option_value(property.value.as_deref(), |encoder, value| {
        encoder.value_str(value);
    });
}

fn encode_change(encoder: &mut Encoder, change: &PlannedChange) {
    encoder.field_str("path", &change.path);
    encoder.field_enum("kind", change_kind(&change.kind));
    encoder.option("content", change.content.as_deref(), |encoder, value| {
        encoder.value_bytes(value);
    });
    encoder.field_bool("executable", change.executable);
    encoder.field_bool("symlink", change.symlink);
    encoder.option("source", change.source.as_ref(), encode_source);
    encoder.sequence("properties", &change.properties, encode_property);
    encoder.option("metadata", change.metadata.as_ref(), encode_metadata);
}

fn encode_source(encoder: &mut Encoder, source: &CopySource) {
    encoder.field_str("path", &source.path);
    encoder.field_u32("revision", source.revision);
}

fn encode_metadata(encoder: &mut Encoder, metadata: &ChangeMetadata) {
    encoder.field_str("old_mode", &metadata.old_mode);
    encoder.field_str("new_mode", &metadata.new_mode);
    encoder.field_str("old_oid", &metadata.old_oid);
    encoder.field_str("new_oid", &metadata.new_oid);
    encoder.option("similarity", metadata.similarity, |encoder, value| {
        encoder.value_u8(value);
    });
}

fn change_kind(kind: &PlannedChangeKind) -> u8 {
    match kind {
        PlannedChangeKind::EnsureDir => 0,
        PlannedChangeKind::AddFile => 1,
        PlannedChangeKind::ModifyFile => 2,
        PlannedChangeKind::Delete => 3,
        PlannedChangeKind::CopyFile => 4,
        PlannedChangeKind::Move => 5,
    }
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new(domain: &str) -> Self {
        let mut encoder = Self { bytes: Vec::new() };
        encoder.value_bytes(domain.as_bytes());
        encoder.value_u32(FORMAT_VERSION);
        encoder
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn structure(&mut self, name: &str, encode: impl FnOnce(&mut Self)) {
        self.field_header(name, 0);
        encode(self);
    }

    fn field_str(&mut self, name: &str, value: &str) {
        self.field_header(name, 1);
        self.value_str(value);
    }

    fn field_path(&mut self, name: &str, value: &Path) {
        self.field_header(name, 2);
        self.value_path(value);
    }

    fn field_u32(&mut self, name: &str, value: u32) {
        self.field_header(name, 3);
        self.value_u32(value);
    }

    fn field_bool(&mut self, name: &str, value: bool) {
        self.field_header(name, 4);
        self.bytes.push(u8::from(value));
    }

    fn field_enum(&mut self, name: &str, value: u8) {
        self.field_header(name, 5);
        self.bytes.push(value);
    }

    fn option<T>(&mut self, name: &str, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        self.field_header(name, 6);
        self.option_value(value, encode);
    }

    fn sequence<T>(&mut self, name: &str, values: &[T], encode: fn(&mut Self, &T)) {
        self.field_header(name, 7);
        self.value_len(values.len());
        for value in values {
            encode(self, value);
        }
    }

    fn field_header(&mut self, name: &str, kind: u8) {
        self.value_bytes(name.as_bytes());
        self.bytes.push(kind);
    }

    fn option_value<T>(&mut self, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        match value {
            Some(value) => {
                self.bytes.push(1);
                encode(self, value);
            }
            None => self.bytes.push(0),
        }
    }

    fn value_str(&mut self, value: &str) {
        self.value_bytes(value.as_bytes());
    }

    fn value_bytes(&mut self, value: &[u8]) {
        self.value_len(value.len());
        self.bytes.extend_from_slice(value);
    }

    fn value_len(&mut self, value: usize) {
        self.bytes.extend_from_slice(&(value as u64).to_be_bytes());
    }

    fn value_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn value_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    #[cfg(unix)]
    fn value_path(&mut self, value: &Path) {
        use std::os::unix::ffi::OsStrExt;

        self.value_bytes(b"unix-bytes");
        self.value_bytes(value.as_os_str().as_bytes());
    }

    #[cfg(windows)]
    fn value_path(&mut self, value: &Path) {
        use std::os::windows::ffi::OsStrExt;

        self.value_bytes(b"windows-utf16");
        let units = value.as_os_str().encode_wide().collect::<Vec<_>>();
        self.value_len(units.len());
        for unit in units {
            self.bytes.extend_from_slice(&unit.to_be_bytes());
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn value_path(&mut self, value: &Path) {
        self.value_bytes(b"utf8-lossy");
        self.value_str(&value.to_string_lossy());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(character: char) -> String {
        character.to_string().repeat(40)
    }

    fn plan() -> DcommitPlan {
        DcommitPlan {
            target: DcommitTarget {
                url: "https://example.invalid/repos/project/trunk".to_owned(),
                repository_root: "https://example.invalid/repos/project".to_owned(),
                repository_uuid: "12345678-1234-1234-1234-123456789abc".to_owned(),
                git_ref: "refs/remotes/origin/trunk".to_owned(),
            },
            base_revision: 40,
            git_commit: oid('b'),
            message: "subject\n\nbody\n".to_owned(),
            author: Some("Test User <test@example.com>".to_owned()),
            root_properties: vec![PropertyChange::set("svn:mergeinfo", "/branches/main:1-2")],
            changes: vec![
                PlannedChange::copy_file("source.bin", 40, "target.bin", [0, 1, 0xff])
                    .with_executable(true)
                    .with_property(PropertyChange::delete("svn:special"))
                    .with_metadata(ChangeMetadata {
                        old_mode: "100644".to_owned(),
                        new_mode: "100755".to_owned(),
                        old_oid: oid('a'),
                        new_oid: oid('b'),
                        similarity: Some(87),
                    }),
            ],
        }
    }

    fn target() -> DcommitTargetIdentity {
        DcommitTargetIdentity {
            remote_id: "svn".to_owned(),
            repository_root_url: "https://example.invalid/repos/project".to_owned(),
            repository_uuid: "12345678-1234-1234-1234-123456789abc".to_owned(),
            mapping_ref: "refs/remotes/origin/trunk".to_owned(),
            rev_map_path: ".git/svn/origin.trunk/.rev_map.uuid".to_owned(),
            commit_url: "https://example.invalid/repos/project/trunk".to_owned(),
        }
    }

    #[test]
    fn fingerprints_are_stable_sha256_hex() {
        let plan = plan();
        assert_eq!(plan_fingerprint(&plan), plan_fingerprint(&plan.clone()));
        assert_eq!(plan_fingerprint(&plan).len(), 64);
        assert_eq!(
            message_fingerprint(&plan.message),
            message_fingerprint("subject\n\nbody\n")
        );
        assert_eq!(message_fingerprint(&plan.message).len(), 64);
    }

    #[test]
    fn plan_fingerprint_changes_for_execution_semantics() {
        let original = plan();
        let expected = plan_fingerprint(&original);
        let mut variants = Vec::new();

        let mut changed = original.clone();
        changed.target.url.push_str("-other");
        variants.push(changed);
        let mut changed = original.clone();
        changed.base_revision += 1;
        variants.push(changed);
        let mut changed = original.clone();
        changed.git_commit = oid('c');
        variants.push(changed);
        let mut changed = original.clone();
        changed.message.push('!');
        variants.push(changed);
        let mut changed = original.clone();
        changed.author = None;
        variants.push(changed);
        let mut changed = original.clone();
        changed.root_properties[0].value = None;
        variants.push(changed);
        let mut changed = original.clone();
        changed.changes[0].path.push_str("-other");
        variants.push(changed);
        let mut changed = original.clone();
        changed.changes[0].kind = PlannedChangeKind::Move;
        variants.push(changed);
        let mut changed = original.clone();
        changed.changes[0].content.as_mut().unwrap().push(2);
        variants.push(changed);
        let mut changed = original.clone();
        changed.changes[0].source.as_mut().unwrap().revision += 1;
        variants.push(changed);
        let mut changed = original.clone();
        changed.changes[0].metadata.as_mut().unwrap().similarity = None;
        variants.push(changed);

        for variant in variants {
            assert_ne!(plan_fingerprint(&variant), expected);
        }
    }

    #[test]
    fn length_prefixes_and_options_are_unambiguous() {
        let mut left = plan();
        left.root_properties = vec![PropertyChange::set("ab", "c")];
        let mut right = plan();
        right.root_properties = vec![PropertyChange::set("a", "bc")];
        assert_ne!(canonical_plan_bytes(&left), canonical_plan_bytes(&right));

        left.author = None;
        right.author = Some(String::new());
        assert_ne!(canonical_plan_bytes(&left), canonical_plan_bytes(&right));
        assert_ne!(message_fingerprint("ab"), message_fingerprint("a\0b"));
    }

    #[test]
    fn recovery_fingerprint_covers_target_path_and_config() {
        let target = target();
        let input = RecoveryFingerprintInput {
            target: &target,
            no_rebase: false,
            mergeinfo: None,
        };
        let expected = recovery_config_fingerprint(input);
        assert_eq!(expected, recovery_config_fingerprint(input));

        let mut changed_target = target.clone();
        changed_target.commit_url.push_str("-other");
        assert_ne!(
            recovery_config_fingerprint(RecoveryFingerprintInput {
                target: &changed_target,
                ..input
            }),
            expected
        );
        let mut changed_target = target.clone();
        changed_target.rev_map_path.push_str("-other");
        assert_ne!(
            recovery_config_fingerprint(RecoveryFingerprintInput {
                target: &changed_target,
                ..input
            }),
            expected
        );
        assert_ne!(
            recovery_config_fingerprint(RecoveryFingerprintInput {
                no_rebase: true,
                ..input
            }),
            expected
        );
        assert_ne!(
            recovery_config_fingerprint(RecoveryFingerprintInput {
                mergeinfo: Some(""),
                ..input
            }),
            expected
        );
    }
}
