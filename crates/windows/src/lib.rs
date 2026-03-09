#[cfg(windows)]
use std::process::Command;

use partbooter_common::{
    AppError, AppErrorKind, AppResult, MachineProbe,
};
#[cfg(any(windows, test))]
use partbooter_common::{EspInfo, FirmwareMode, HostPlatform, PartitionStyle};

#[cfg(windows)]
const PROBE_SCRIPT: &str = r#"
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$warnings = @()

$firmwareMode = "bios"
try {
    $firmwareType = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control' -Name 'PEFirmwareType' -ErrorAction Stop).PEFirmwareType
    if ($firmwareType -eq 2) {
        $firmwareMode = "uefi"
    }
}
catch {
    $warnings += "Unable to determine firmware mode from PEFirmwareType."
}

$systemDisk = Get-Disk -ErrorAction SilentlyContinue | Where-Object { ($_.IsBoot -eq $true) -or ($_.IsSystem -eq $true) } | Select-Object -First 1
$partitionStyle = "unknown"
if ($systemDisk) {
    $partitionStyle = $systemDisk.PartitionStyle.ToString().ToLowerInvariant()
} else {
    $warnings += "Unable to locate the system disk."
}

$secureBootEnabled = $false
try {
    $secureBootEnabled = [bool](Confirm-SecureBootUEFI)
}
catch {
    $warnings += "Unable to confirm Secure Boot state."
}

$bitlockerDetected = $false
if (Get-Command Get-BitLockerVolume -ErrorAction SilentlyContinue) {
    $bitlockerVolumes = @(Get-BitLockerVolume -ErrorAction SilentlyContinue | Where-Object { ($_.ProtectionStatus -eq 'On') -or ($_.VolumeStatus -eq 'FullyEncrypted') -or ($_.VolumeStatus -eq 'EncryptionInProgress') })
    if ($bitlockerVolumes -and ($bitlockerVolumes.Count -gt 0)) {
        $bitlockerDetected = $true
    }
} else {
    $warnings += "BitLocker cmdlet unavailable; encryption state may be incomplete."
}

$espGuid = '{C12A7328-F81F-11D2-BA4B-00A0C93EC93B}'
$espPartition = $null
if ($systemDisk) {
    $espPartition = Get-Partition -DiskNumber $systemDisk.Number -ErrorAction SilentlyContinue | Where-Object { $_.GptType -eq $espGuid } | Select-Object -First 1
}
if (-not $espPartition) {
    $espPartition = Get-Partition -ErrorAction SilentlyContinue | Where-Object { $_.GptType -eq $espGuid } | Select-Object -First 1
}

$espVolume = ""
$espMountPoint = ""
$espFilesystem = ""
$espFreeSpaceMb = 0

if ($espPartition) {
    $accessPaths = @($espPartition.AccessPaths)
    if ($accessPaths -and ($accessPaths.Count -gt 0)) {
        $espMountPoint = [string]$accessPaths[0]
        $espVolume = [string]$accessPaths[0]
    }

    $volume = Get-Volume -Partition $espPartition -ErrorAction SilentlyContinue
    if ($volume) {
        if (-not $espVolume) {
            $espVolume = [string]$volume.Path
        }
        $driveLetter = $volume.DriveLetter
        if ((-not $espMountPoint) -and $driveLetter) {
            $espMountPoint = [string]::Format("{0}:\", $driveLetter)
        }
        $espFilesystem = [string]$volume.FileSystem
        $espFreeSpaceMb = [int64]([math]::Floor($volume.SizeRemaining / 1MB))
    } else {
        $warnings += "Unable to resolve the EFI System Partition volume metadata."
    }
} else {
    $warnings += "Unable to locate the EFI System Partition."
}

$supported = $true
if ($firmwareMode -ne "uefi") {
    $supported = $false
    $warnings += "Firmware mode is not UEFI."
}
if ($partitionStyle -ne "gpt") {
    $supported = $false
    $warnings += "System disk partition style is not GPT."
}
if (-not $espPartition) {
    $supported = $false
}
if (-not $espVolume) {
    $supported = $false
    $warnings += "Unable to resolve the EFI System Partition volume path."
}

Write-Output "host_platform=windows"
Write-Output ("firmware_mode=" + $firmwareMode)
Write-Output ("partition_style=" + $partitionStyle)
Write-Output ("secure_boot_enabled=" + $secureBootEnabled.ToString().ToLowerInvariant())
Write-Output ("bitlocker_detected=" + $bitlockerDetected.ToString().ToLowerInvariant())
Write-Output ("esp_volume=" + $espVolume)
Write-Output ("esp_mount_point=" + $espMountPoint)
Write-Output ("esp_filesystem=" + $espFilesystem)
Write-Output ("esp_free_space_mb=" + $espFreeSpaceMb.ToString())
foreach ($warning in $warnings) {
    Write-Output ("warning=" + $warning)
}
Write-Output ("supported=" + $supported.ToString().ToLowerInvariant())
"#;

pub struct WindowsProbeAdapter;

impl WindowsProbeAdapter {
    pub fn probe() -> AppResult<MachineProbe> {
        probe_impl()
    }
}

#[cfg(windows)]
fn probe_impl() -> AppResult<MachineProbe> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            PROBE_SCRIPT,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        let message = if detail.is_empty() {
            format!(
                "Windows probe command failed with exit status {}",
                output.status
            )
        } else {
            format!("Windows probe command failed: {detail}")
        };
        return Err(AppError::new(AppErrorKind::Io, message));
    }

    parse_probe_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn probe_impl() -> AppResult<MachineProbe> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter live probing only runs on Windows hosts",
    ))
}

#[cfg(any(windows, test))]
fn parse_probe_output(output: &str) -> AppResult<MachineProbe> {
    let mut host_platform = None;
    let mut firmware_mode = None;
    let mut partition_style = None;
    let mut secure_boot_enabled = None;
    let mut bitlocker_detected = None;
    let mut esp_volume = None;
    let mut esp_mount_point = None;
    let mut esp_filesystem = None;
    let mut esp_free_space_mb = None;
    let mut warnings = Vec::new();
    let mut supported = None;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let (key, value) = line.split_once('=').ok_or_else(|| {
            AppError::new(
                AppErrorKind::Validation,
                format!("invalid probe output line: {line}"),
            )
        })?;

        match key {
            "host_platform" => host_platform = Some(value.to_string()),
            "firmware_mode" => firmware_mode = Some(parse_firmware_mode(value)?),
            "partition_style" => partition_style = Some(parse_partition_style(value)),
            "secure_boot_enabled" => secure_boot_enabled = Some(parse_bool(value, key)?),
            "bitlocker_detected" => bitlocker_detected = Some(parse_bool(value, key)?),
            "esp_volume" => esp_volume = Some(value.to_string()),
            "esp_mount_point" => esp_mount_point = Some(value.to_string()),
            "esp_filesystem" => esp_filesystem = Some(value.to_string()),
            "esp_free_space_mb" => {
                esp_free_space_mb = Some(value.parse::<u64>().map_err(|_| {
                    AppError::new(
                        AppErrorKind::Validation,
                        format!("invalid esp_free_space_mb value: {value}"),
                    )
                })?)
            }
            "warning" => warnings.push(value.to_string()),
            "supported" => supported = Some(parse_bool(value, key)?),
            _ => {}
        }
    }

    let host_platform = host_platform.unwrap_or_default();
    if host_platform != "windows" {
        return Err(AppError::new(
            AppErrorKind::Validation,
            format!("unexpected host platform from probe output: {host_platform}"),
        ));
    }

    Ok(MachineProbe {
        host_platform: HostPlatform::Windows,
        firmware_mode: firmware_mode.ok_or_else(|| missing_field("firmware_mode"))?,
        partition_style: partition_style.ok_or_else(|| missing_field("partition_style"))?,
        secure_boot_enabled: secure_boot_enabled
            .ok_or_else(|| missing_field("secure_boot_enabled"))?,
        bitlocker_detected: bitlocker_detected
            .ok_or_else(|| missing_field("bitlocker_detected"))?,
        esp: EspInfo {
            volume: esp_volume.ok_or_else(|| missing_field("esp_volume"))?,
            mount_point: esp_mount_point.ok_or_else(|| missing_field("esp_mount_point"))?,
            filesystem: esp_filesystem.ok_or_else(|| missing_field("esp_filesystem"))?,
            free_space_mb: esp_free_space_mb.ok_or_else(|| missing_field("esp_free_space_mb"))?,
        },
        warnings,
        supported: supported.ok_or_else(|| missing_field("supported"))?,
    })
}

#[cfg(any(windows, test))]
fn parse_firmware_mode(value: &str) -> AppResult<FirmwareMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "uefi" => Ok(FirmwareMode::Uefi),
        "bios" => Ok(FirmwareMode::Bios),
        _ => Err(AppError::new(
            AppErrorKind::Validation,
            format!("invalid firmware_mode value: {value}"),
        )),
    }
}

#[cfg(any(windows, test))]
fn parse_partition_style(value: &str) -> PartitionStyle {
    match value.trim().to_ascii_lowercase().as_str() {
        "gpt" => PartitionStyle::Gpt,
        "mbr" => PartitionStyle::Mbr,
        _ => PartitionStyle::Unknown,
    }
}

#[cfg(any(windows, test))]
fn parse_bool(value: &str, field_name: &str) -> AppResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(AppError::new(
            AppErrorKind::Validation,
            format!("invalid {field_name} value: {value}"),
        )),
    }
}

#[cfg(any(windows, test))]
fn missing_field(name: &str) -> AppError {
    AppError::new(
        AppErrorKind::Validation,
        format!("missing probe output field: {name}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{WindowsProbeAdapter, parse_probe_output};
    use partbooter_common::{FirmwareMode, PartitionStyle};

    #[test]
    fn parses_supported_probe_output() {
        let parsed = parse_probe_output(
            "host_platform=windows\nfirmware_mode=uefi\npartition_style=gpt\nsecure_boot_enabled=true\nbitlocker_detected=false\nesp_volume=\\\\?\\Volume{ABC}\nesp_mount_point=S:\\\nesp_filesystem=FAT32\nesp_free_space_mb=512\nwarning=BitLocker cmdlet unavailable; encryption state may be incomplete.\nsupported=true\n",
        )
        .expect("probe output should parse");

        assert_eq!(parsed.firmware_mode, FirmwareMode::Uefi);
        assert_eq!(parsed.partition_style, PartitionStyle::Gpt);
        assert_eq!(parsed.esp.filesystem, "FAT32");
        assert!(parsed.supported);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn parses_blocked_probe_output() {
        let parsed = parse_probe_output(
            "host_platform=windows\nfirmware_mode=bios\npartition_style=mbr\nsecure_boot_enabled=false\nbitlocker_detected=true\nesp_volume=\nesp_mount_point=\nesp_filesystem=\nesp_free_space_mb=0\nwarning=Firmware mode is not UEFI.\nwarning=System disk partition style is not GPT.\nsupported=false\n",
        )
        .expect("probe output should parse");

        assert_eq!(parsed.firmware_mode, FirmwareMode::Bios);
        assert_eq!(parsed.partition_style, PartitionStyle::Mbr);
        assert!(parsed.bitlocker_detected);
        assert!(!parsed.supported);
    }

    #[cfg(not(windows))]
    #[test]
    fn live_probe_fails_on_non_windows_hosts() {
        let error = WindowsProbeAdapter::probe().expect_err("non-Windows hosts should fail");
        assert_eq!(error.exit_code(), 2);
    }
}
