use std::path::Path;

use partbooter_common::{PayloadKind, PayloadSpec};

pub fn detect(path: &Path) -> Option<PayloadSpec> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if extension != "iso" {
        return None;
    }

    let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    let (display_name, profile, supported, note) = if file_name.contains("ubuntu") {
        (
            "Ubuntu Live".to_string(),
            "ubuntu-live".to_string(),
            true,
            "Profile supports the v1 Linux ISO loopback path.".to_string(),
        )
    } else if file_name.contains("debian") {
        (
            "Debian Live".to_string(),
            "debian-live".to_string(),
            true,
            "Profile supports the v1 Linux ISO loopback path.".to_string(),
        )
    } else if file_name.contains("fedora") {
        (
            "Fedora Live".to_string(),
            "fedora-live".to_string(),
            true,
            "Profile supports the v1 Linux ISO loopback path.".to_string(),
        )
    } else {
        (
            "Unknown Linux ISO".to_string(),
            "unsupported-linux-iso".to_string(),
            false,
            "This ISO is not in the supported Linux profile list for v1.".to_string(),
        )
    };

    Some(PayloadSpec {
        source_path: path.display().to_string(),
        kind: PayloadKind::LinuxIso,
        display_name,
        profile,
        supported,
        notes: vec![note],
    })
}

#[cfg(test)]
mod tests {
    use super::detect;
    use partbooter_common::PayloadKind;
    use std::path::Path;

    #[test]
    fn detects_supported_ubuntu_iso() {
        let detected = detect(Path::new("C:\\images\\ubuntu-24.04.iso")).expect("iso expected");
        assert_eq!(detected.kind, PayloadKind::LinuxIso);
        assert!(detected.supported);
        assert_eq!(detected.profile, "ubuntu-live");
    }

    #[test]
    fn rejects_unknown_profile_as_unsupported() {
        let detected = detect(Path::new("C:\\images\\random.iso")).expect("iso expected");
        assert!(!detected.supported);
        assert_eq!(detected.profile, "unsupported-linux-iso");
    }
}
