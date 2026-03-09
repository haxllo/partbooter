use std::path::Path;

use partbooter_common::{PayloadKind, PayloadSpec};

pub fn detect(path: &Path) -> Option<PayloadSpec> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if extension != "wim" {
        return None;
    }

    let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    let supported = file_name.contains("winpe") || file_name.contains("boot");

    Some(PayloadSpec {
        source_path: path.display().to_string(),
        kind: PayloadKind::WinPe,
        display_name: "Windows PE".to_string(),
        profile: if supported {
            "winpe-wim".to_string()
        } else {
            "unsupported-winpe".to_string()
        },
        supported,
        notes: vec![if supported {
            "WinPE WIM payload follows the controlled v1 boot path.".to_string()
        } else {
            "Only WinPE-style WIM payloads are supported in v1.".to_string()
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::detect;
    use partbooter_common::PayloadKind;
    use std::path::Path;

    #[test]
    fn detects_supported_winpe_wim() {
        let detected = detect(Path::new("C:\\images\\winpe_boot.wim")).expect("wim expected");
        assert_eq!(detected.kind, PayloadKind::WinPe);
        assert!(detected.supported);
        assert_eq!(detected.profile, "winpe-wim");
    }
}
