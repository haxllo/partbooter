use partbooter_common::{EspInfo, FirmwareMode, HostPlatform, MachineProbe, PartitionStyle};

pub struct WindowsProbeAdapter;

impl WindowsProbeAdapter {
    pub fn probe() -> MachineProbe {
        MachineProbe {
            host_platform: HostPlatform::Windows,
            firmware_mode: FirmwareMode::Uefi,
            partition_style: PartitionStyle::Gpt,
            secure_boot_enabled: true,
            bitlocker_detected: false,
            esp: EspInfo {
                volume: "\\\\?\\Volume{ESP}".to_string(),
                mount_point: "S:\\".to_string(),
                filesystem: "FAT32".to_string(),
                free_space_mb: 512,
            },
            warnings: vec![
                "Probe adapter is currently a deterministic scaffold and must be replaced with real Windows inspection.".to_string(),
            ],
            supported: true,
        }
    }
}
