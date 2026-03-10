#[cfg(windows)]
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;

use partbooter_common::{AppError, AppErrorKind, AppResult, EspInfo, MachineProbe};
#[cfg(any(windows, test))]
use partbooter_common::{FirmwareMode, HostPlatform, PartitionStyle};

#[cfg(windows)]
const PROBE_SCRIPT: &str = r#"
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$warnings = @()

$firmwareMode = "bios"
$firmwareRegistryUnavailable = $false
try {
    $firmwareType = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control' -Name 'PEFirmwareType' -ErrorAction Stop).PEFirmwareType
    if ($firmwareType -eq 2) {
        $firmwareMode = "uefi"
    }
}
catch {
    $firmwareRegistryUnavailable = $true
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

if ($firmwareMode -ne "uefi") {
    $currentBootEntry = @(bcdedit /enum '{current}' 2>$null)
    if (($LASTEXITCODE -eq 0) -and ($currentBootEntry.Count -gt 0)) {
        $loaderPathLine = $currentBootEntry | Where-Object { $_ -match '^\s*path\s+' } | Select-Object -First 1
        if ($loaderPathLine -match 'winload\.efi') {
            $firmwareMode = "uefi"
        } elseif ($loaderPathLine -match 'winload\.exe') {
            $firmwareMode = "bios"
        }
    }
}

if (($firmwareMode -ne "uefi") -and ($partitionStyle -eq "gpt") -and $espPartition) {
    $firmwareMode = "uefi"
    $warnings += "Firmware mode inferred from GPT system disk and EFI System Partition."
}

if (($firmwareMode -ne "uefi") -and $firmwareRegistryUnavailable) {
    $warnings += "Unable to determine firmware mode from PEFirmwareType or current boot loader."
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

#[derive(Debug, Clone)]
pub struct BackupCheckpoint {
    pub esp_backup_dir: PathBuf,
    pub bcd_store_path: PathBuf,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WinPeStaging {
    pub stage_root: PathBuf,
    pub boot_wim_path: PathBuf,
    pub boot_sdi_path: PathBuf,
    pub target_volume: String,
}

#[derive(Debug, Clone)]
pub struct BootEntryRegistration {
    pub entry_id: String,
    pub ramdisk_options_id: String,
    pub display_name: String,
}

pub struct WindowsApplyAdapter;

impl WindowsApplyAdapter {
    pub fn create_backup_checkpoint(
        esp: &EspInfo,
        backup_root: impl AsRef<Path>,
    ) -> AppResult<BackupCheckpoint> {
        create_backup_checkpoint_impl(esp, backup_root.as_ref())
    }

    pub fn stage_winpe_payload(
        source_wim: impl AsRef<Path>,
        target_volume: &str,
        operation_id: &str,
    ) -> AppResult<WinPeStaging> {
        stage_winpe_payload_impl(source_wim.as_ref(), target_volume, operation_id)
    }

    pub fn register_winpe_boot_entry(
        staging: &WinPeStaging,
        display_name: &str,
    ) -> AppResult<BootEntryRegistration> {
        register_winpe_boot_entry_impl(staging, display_name)
    }

    pub fn verify_boot_entry(entry_id: &str) -> AppResult<bool> {
        verify_boot_entry_impl(entry_id)
    }

    pub fn remove_boot_entry(entry_id: &str, ramdisk_options_id: &str) -> AppResult<()> {
        remove_boot_entry_impl(entry_id, ramdisk_options_id)
    }

    pub fn remove_staged_payload(stage_root: impl AsRef<Path>) -> AppResult<()> {
        remove_staged_payload_impl(stage_root.as_ref())
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

#[cfg(windows)]
fn create_backup_checkpoint_impl(esp: &EspInfo, backup_root: &Path) -> AppResult<BackupCheckpoint> {
    fs::create_dir_all(backup_root)?;

    let esp_backup_dir = backup_root.join("esp");
    fs::create_dir_all(&esp_backup_dir)?;
    let bcd_store_path = backup_root.join("bcd-store.bak");

    let mut mounted_drive = None;
    let esp_source = if is_usable_mount_point(&esp.mount_point) {
        normalize_root_path(&esp.mount_point)
    } else {
        let drive_letter = find_free_drive_letter()?;
        mount_esp_to_letter(drive_letter)?;
        let mounted = format!("{drive_letter}:\\");
        mounted_drive = Some(mounted.clone());
        PathBuf::from(mounted)
    };

    let robocopy_code = Command::new("robocopy")
        .arg(&esp_source)
        .arg(&esp_backup_dir)
        .args(esp_backup_robocopy_args())
        .status()?
        .code()
        .unwrap_or(16);

    if let Some(drive) = mounted_drive {
        let _ = unmount_esp_from_letter(&drive);
    }

    if !robocopy_succeeded(robocopy_code) {
        return Err(AppError::new(
            AppErrorKind::Io,
            format!("robocopy failed while backing up the ESP with exit code {robocopy_code}"),
        ));
    }

    let bcd_status = Command::new("bcdedit")
        .args(["/export", &bcd_store_path.display().to_string()])
        .status()?;
    if !bcd_status.success() {
        return Err(AppError::new(
            AppErrorKind::Io,
            format!("bcdedit /export failed with exit status {bcd_status}"),
        ));
    }

    Ok(BackupCheckpoint {
        esp_backup_dir,
        bcd_store_path,
        notes: vec![
            "ESP backup created with robocopy; live BCD files were excluded because bcdedit exported the store separately.".to_string(),
            "BCD snapshot exported with bcdedit.".to_string(),
        ],
    })
}

#[cfg(not(windows))]
fn create_backup_checkpoint_impl(
    _esp: &EspInfo,
    _backup_root: &Path,
) -> AppResult<BackupCheckpoint> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter backup checkpointing only runs on Windows hosts",
    ))
}

#[cfg(windows)]
fn stage_winpe_payload_impl(
    source_wim: &Path,
    target_volume: &str,
    operation_id: &str,
) -> AppResult<WinPeStaging> {
    let target_root = normalize_volume_root(target_volume)?;
    let stage_root = target_root
        .join("PartBooter")
        .join("Operations")
        .join(operation_id)
        .join("WinPE");
    fs::create_dir_all(&stage_root)?;

    let boot_wim_path = stage_root.join("boot.wim");
    fs::copy(source_wim, &boot_wim_path).map_err(|error| {
        AppError::new(
            AppErrorKind::Io,
            format!(
                "failed to stage WinPE WIM from {} to {}: {error}",
                source_wim.display(),
                boot_wim_path.display()
            ),
        )
    })?;

    let boot_sdi_source = locate_boot_sdi_source()?;
    let boot_sdi_path = stage_root.join("boot.sdi");
    fs::copy(&boot_sdi_source, &boot_sdi_path).map_err(|error| {
        AppError::new(
            AppErrorKind::Io,
            format!(
                "failed to stage boot.sdi from {} to {}: {error}",
                boot_sdi_source.display(),
                boot_sdi_path.display()
            ),
        )
    })?;

    Ok(WinPeStaging {
        stage_root,
        boot_wim_path,
        boot_sdi_path,
        target_volume: normalized_volume_token(target_volume)?,
    })
}

#[cfg(not(windows))]
fn stage_winpe_payload_impl(
    _source_wim: &Path,
    _target_volume: &str,
    _operation_id: &str,
) -> AppResult<WinPeStaging> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter WinPE staging only runs on Windows hosts",
    ))
}

#[cfg(windows)]
fn register_winpe_boot_entry_impl(
    staging: &WinPeStaging,
    display_name: &str,
) -> AppResult<BootEntryRegistration> {
    let entry_id = create_bcd_object(display_name, "osloader")?;
    let ramdisk_options_id = create_device_options_object(&format!("{display_name} options"))?;
    let registration = (|| {
        let volume_token = &staging.target_volume;
        let ramdisk_sdi_path = windows_volume_relative_path(&staging.boot_sdi_path, volume_token)?;
        let ramdisk_wim_path = windows_volume_relative_path(&staging.boot_wim_path, volume_token)?;
        let volume_spec =
            configure_ramdisk_options(&ramdisk_options_id, &ramdisk_sdi_path, volume_token)?;
        let ramdisk_device_base =
            format!("ramdisk={}{}", volume_spec.ramdisk_prefix, ramdisk_wim_path);
        let ramdisk_device = format!("{ramdisk_device_base},{ramdisk_options_id}");

        set_bcd_value(&entry_id, "device", &ramdisk_device)?;
        set_bcd_value(&entry_id, "osdevice", &ramdisk_device)?;
        set_bcd_value(&entry_id, "path", r"\Windows\System32\winload.efi")?;
        set_bcd_value(&entry_id, "systemroot", r"\Windows")?;
        set_bcd_value(&entry_id, "winpe", "yes")?;
        set_bcd_value(&entry_id, "detecthal", "yes")?;
        set_bcd_value(&entry_id, "nx", "OptIn")?;

        add_bcd_display_order(&entry_id)?;

        Ok(BootEntryRegistration {
            entry_id: entry_id.clone(),
            ramdisk_options_id: ramdisk_options_id.clone(),
            display_name: display_name.to_string(),
        })
    })();

    if let Err(error) = registration {
        let _ = delete_bcd_object(&entry_id);
        let _ = delete_bcd_object(&ramdisk_options_id);
        return Err(error);
    }

    registration
}

#[cfg(not(windows))]
fn register_winpe_boot_entry_impl(
    _staging: &WinPeStaging,
    _display_name: &str,
) -> AppResult<BootEntryRegistration> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter WinPE boot entry registration only runs on Windows hosts",
    ))
}

#[cfg(windows)]
fn verify_boot_entry_impl(entry_id: &str) -> AppResult<bool> {
    let output = Command::new("bcdedit").args(["/enum", entry_id]).output()?;
    Ok(output.status.success())
}

#[cfg(not(windows))]
fn verify_boot_entry_impl(_entry_id: &str) -> AppResult<bool> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter BCD verification only runs on Windows hosts",
    ))
}

#[cfg(windows)]
fn remove_boot_entry_impl(entry_id: &str, ramdisk_options_id: &str) -> AppResult<()> {
    delete_bcd_object(entry_id)?;
    delete_bcd_object(ramdisk_options_id)?;
    Ok(())
}

#[cfg(not(windows))]
fn remove_boot_entry_impl(_entry_id: &str, _ramdisk_options_id: &str) -> AppResult<()> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter BCD rollback only runs on Windows hosts",
    ))
}

#[cfg(windows)]
fn remove_staged_payload_impl(stage_root: &Path) -> AppResult<()> {
    if stage_root.exists() {
        fs::remove_dir_all(stage_root).map_err(|error| {
            AppError::new(
                AppErrorKind::Io,
                format!(
                    "failed to remove staged WinPE payload at {}: {error}",
                    stage_root.display()
                ),
            )
        })?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn remove_staged_payload_impl(_stage_root: &Path) -> AppResult<()> {
    Err(AppError::new(
        AppErrorKind::UnsupportedEnvironment,
        "PartBooter staged-payload cleanup only runs on Windows hosts",
    ))
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct BcdVolumeSpec {
    partition_device: String,
    ramdisk_prefix: String,
}

#[cfg(windows)]
fn is_usable_mount_point(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.len() >= 3 && trimmed.as_bytes().get(1) == Some(&b':')
}

#[cfg(windows)]
fn normalize_volume_root(volume: &str) -> AppResult<PathBuf> {
    let token = normalized_volume_token(volume)?;
    Ok(PathBuf::from(format!("{token}\\")))
}

#[cfg(windows)]
fn normalized_volume_token(volume: &str) -> AppResult<String> {
    let trimmed = volume.trim().trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':') {
        Ok(trimmed.to_string())
    } else {
        Err(AppError::new(
            AppErrorKind::Validation,
            format!("invalid target volume; expected a drive root like D:, got {volume}"),
        ))
    }
}

#[cfg(windows)]
fn locate_boot_sdi_source() -> AppResult<PathBuf> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let candidates = [
        PathBuf::from(format!(r"{windir}\Boot\DVD\EFI\boot.sdi")),
        PathBuf::from(format!(r"{windir}\Boot\DVD\PCAT\boot.sdi")),
        PathBuf::from(format!(r"{windir}\Boot\PXE\boot.sdi")),
        PathBuf::from(format!(r"{windir}\System32\Recovery\boot.sdi")),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(AppError::new(
        AppErrorKind::Io,
        "unable to locate boot.sdi on the host; checked common Windows boot paths",
    ))
}

#[cfg(windows)]
fn configure_ramdisk_options(
    ramdisk_options_id: &str,
    ramdisk_sdi_path: &str,
    volume_token: &str,
) -> AppResult<BcdVolumeSpec> {
    let mut last_error = None;
    for spec in candidate_bcd_volume_specs(volume_token) {
        match set_bcd_value(
            ramdisk_options_id,
            "ramdisksdidevice",
            &spec.partition_device,
        ) {
            Ok(()) => {
                set_bcd_value(ramdisk_options_id, "ramdisksdipath", ramdisk_sdi_path)?;
                return Ok(spec);
            }
            Err(error) => {
                last_error = Some(AppError::new(
                    error.kind(),
                    format!(
                        "failed to set ramdisksdidevice using {}: {}",
                        spec.partition_device,
                        error.message()
                    ),
                ));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        AppError::new(
            AppErrorKind::Io,
            "no candidate BCD volume device syntax was available",
        )
    }))
}

#[cfg(windows)]
fn create_bcd_object(description: &str, application: &str) -> AppResult<String> {
    let output = Command::new("bcdedit")
        .args(["/create", "/d", description, "/application", application])
        .output()?;
    if !output.status.success() {
        return Err(AppError::new(
            AppErrorKind::Io,
            format!(
                "bcdedit /create for {description} failed with exit status {}",
                output.status
            ),
        ));
    }
    parse_guid_from_bcd_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(windows)]
fn create_device_options_object(description: &str) -> AppResult<String> {
    let output = Command::new("bcdedit")
        .args(["/create", "/d", description, "/device"])
        .output()?;
    if !output.status.success() {
        return Err(AppError::new(
            AppErrorKind::Io,
            format!(
                "bcdedit /create /device for {description} failed with exit status {}",
                output.status
            ),
        ));
    }
    parse_guid_from_bcd_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(windows)]
fn set_bcd_value(entry_id: &str, key: &str, value: &str) -> AppResult<()> {
    run_bcdedit_status(
        ["/set", entry_id, key, value],
        &format!("set {key} on {entry_id}"),
    )
}

#[cfg(windows)]
fn add_bcd_display_order(entry_id: &str) -> AppResult<()> {
    run_bcdedit_status(
        ["/displayorder", entry_id, "/addlast"],
        &format!("add {entry_id} to display order"),
    )
}

#[cfg(windows)]
fn run_bcdedit_status<const N: usize>(args: [&str; N], action: &str) -> AppResult<()> {
    let output = Command::new("bcdedit").args(args).output()?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = join_command_output(&output.stdout, &output.stderr);
        Err(AppError::new(
            AppErrorKind::Io,
            if detail.is_empty() {
                format!(
                    "bcdedit failed to {action} with exit status {}",
                    output.status
                )
            } else {
                format!(
                    "bcdedit failed to {action} with exit status {}: {detail}",
                    output.status
                )
            },
        ))
    }
}

#[cfg(windows)]
fn delete_bcd_object(entry_id: &str) -> AppResult<()> {
    run_bcdedit_status(
        ["/delete", entry_id],
        &format!("delete BCD object {entry_id}"),
    )
}

#[cfg(windows)]
fn parse_guid_from_bcd_output(output: &str) -> AppResult<String> {
    let start = output.find('{').ok_or_else(|| {
        AppError::new(
            AppErrorKind::Validation,
            "could not find a GUID in bcdedit output",
        )
    })?;
    let end = output[start..].find('}').ok_or_else(|| {
        AppError::new(
            AppErrorKind::Validation,
            "could not find the end of the GUID in bcdedit output",
        )
    })?;
    Ok(output[start..=start + end].trim().to_string())
}

#[cfg(any(windows, test))]
fn candidate_bcd_volume_specs(volume_token: &str) -> Vec<BcdVolumeSpec> {
    let mut candidates = Vec::new();

    if let Some(native_device) = resolve_native_volume_device(volume_token) {
        candidates.push(BcdVolumeSpec {
            partition_device: format!("partition={native_device}"),
            ramdisk_prefix: format!("[{native_device}]"),
        });
    }

    candidates.push(BcdVolumeSpec {
        partition_device: format!("partition={volume_token}"),
        ramdisk_prefix: format!("[{volume_token}]"),
    });

    candidates
}

#[cfg(windows)]
fn resolve_native_volume_device(volume_token: &str) -> Option<String> {
    let aliases = volume_aliases(volume_token);
    let output = Command::new("fltmc").arg("volumes").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_fltmc_volume_device(&String::from_utf8_lossy(&output.stdout), &aliases)
}

#[cfg(not(windows))]
#[cfg_attr(not(test), allow(dead_code))]
fn resolve_native_volume_device(_volume_token: &str) -> Option<String> {
    None
}

#[cfg(any(windows, test))]
fn parse_fltmc_volume_device(output: &str, aliases: &[String]) -> Option<String> {
    let mut current_device = None::<String>;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with("Filter Manager")
            || line.starts_with("Volume Name")
            || line.starts_with('-')
        {
            continue;
        }

        if let Some(device) = parse_native_device_from_line(line) {
            if line_mentions_volume_alias(line, aliases) {
                return Some(device.to_string());
            }
            current_device = Some(device.to_string());
            continue;
        }

        if line_mentions_volume_alias(line, aliases) {
            if let Some(device) = &current_device {
                return Some(device.clone());
            }
        }
    }

    None
}

#[cfg(any(windows, test))]
fn parse_native_device_from_line(line: &str) -> Option<&str> {
    let token = line.split_whitespace().next()?;
    if token.starts_with(r"\Device\") {
        Some(token)
    } else {
        None
    }
}

#[cfg(any(windows, test))]
fn line_mentions_volume_alias(line: &str, aliases: &[String]) -> bool {
    line.split_whitespace().any(|segment| {
        let normalized_segment =
            segment.trim_matches(|c| matches!(c, '(' | ')' | ',' | '\\' | '/'));
        aliases
            .iter()
            .any(|alias| normalized_segment.eq_ignore_ascii_case(alias))
    })
}

#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
fn normalize_volume_alias(alias: &str) -> String {
    alias
        .trim()
        .trim_matches(|c| matches!(c, '(' | ')' | ',' | '\\' | '/'))
        .to_string()
}

#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
fn volume_aliases(volume_token: &str) -> Vec<String> {
    let mut aliases = vec![normalize_volume_alias(volume_token)];

    if let Some(guid_path) = resolve_volume_guid_path(volume_token) {
        let normalized_guid = normalize_volume_alias(&guid_path);
        if !normalized_guid.is_empty()
            && !aliases
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&normalized_guid))
        {
            aliases.push(normalized_guid);
        }
    }

    aliases
}

#[cfg(windows)]
fn resolve_volume_guid_path(volume_token: &str) -> Option<String> {
    let mount_target = format!("{volume_token}\\");
    let output = Command::new("mountvol")
        .args([mount_target.as_str(), "/L"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(r"\\?\Volume{"))
        .map(str::to_string)
}

#[cfg(not(windows))]
#[cfg_attr(not(test), allow(dead_code))]
fn resolve_volume_guid_path(_volume_token: &str) -> Option<String> {
    None
}

#[cfg(windows)]
fn join_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout_text = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(stderr).trim().to_string();

    match (stdout_text.is_empty(), stderr_text.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout_text,
        (true, false) => stderr_text,
        (false, false) => format!("{stdout_text} | {stderr_text}"),
    }
}

#[cfg(windows)]
fn windows_volume_relative_path(path: &Path, volume_token: &str) -> AppResult<String> {
    let path_str = path.display().to_string();
    let prefix = format!("{volume_token}\\");
    if let Some(relative) = path_str.strip_prefix(&prefix) {
        Ok(format!("\\{}", relative.replace('/', "\\")))
    } else {
        Err(AppError::new(
            AppErrorKind::Validation,
            format!(
                "path {} is not rooted under the expected target volume {}",
                path.display(),
                volume_token
            ),
        ))
    }
}

#[cfg(windows)]
fn normalize_root_path(path: &str) -> PathBuf {
    if path.ends_with('\\') || path.ends_with('/') {
        PathBuf::from(path)
    } else {
        PathBuf::from(format!("{path}\\"))
    }
}

#[cfg(windows)]
fn find_free_drive_letter() -> AppResult<char> {
    for letter in ('T'..='Z').rev() {
        let candidate = format!("{letter}:\\");
        if !Path::new(&candidate).exists() {
            return Ok(letter);
        }
    }

    Err(AppError::new(
        AppErrorKind::Io,
        "unable to find a temporary drive letter for mounting the ESP",
    ))
}

#[cfg(windows)]
fn mount_esp_to_letter(letter: char) -> AppResult<()> {
    let mount_target = format!("{letter}:");
    let status = Command::new("mountvol")
        .args([&mount_target, "/s"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            AppErrorKind::Io,
            format!("mountvol {mount_target} /s failed with exit status {status}"),
        ))
    }
}

#[cfg(windows)]
fn unmount_esp_from_letter(drive: &str) -> AppResult<()> {
    let target = drive.trim_end_matches('\\');
    let status = Command::new("mountvol").args([target, "/d"]).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            AppErrorKind::Io,
            format!("mountvol {target} /d failed with exit status {status}"),
        ))
    }
}

#[cfg(any(windows, test))]
fn robocopy_succeeded(code: i32) -> bool {
    (0..=7).contains(&code)
}

#[cfg(any(windows, test))]
fn esp_backup_robocopy_args() -> [&'static str; 16] {
    [
        "/E",
        "/COPY:DAT",
        "/R:1",
        "/W:1",
        "/NFL",
        "/NDL",
        "/NJH",
        "/NJS",
        "/NP",
        "/XF",
        "BCD",
        "BCD.LOG",
        "BCD.LOG1",
        "BCD.LOG2",
        "BCD.LOG*",
        "BCD.TMP",
    ]
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
    #[cfg(not(windows))]
    use super::WindowsProbeAdapter;
    use super::{
        candidate_bcd_volume_specs, parse_fltmc_volume_device, parse_probe_output,
        robocopy_succeeded,
    };
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

    #[test]
    fn robocopy_exit_code_rules_match_windows_convention() {
        assert!(robocopy_succeeded(0));
        assert!(robocopy_succeeded(7));
        assert!(!robocopy_succeeded(8));
    }

    #[test]
    fn esp_backup_excludes_live_bcd_files() {
        let args = super::esp_backup_robocopy_args();
        assert!(args.contains(&"/XF"));
        assert!(args.contains(&"BCD"));
        assert!(args.contains(&"BCD.LOG"));
        assert!(args.contains(&"BCD.LOG*"));
    }

    #[test]
    fn parses_native_volume_device_from_fltmc_output() {
        let output = r#"
Filter Manager volumes
----------------------
Volume Name                                Altitude   Frame
-----------------------------------------  --------   -----
\Device\HarddiskVolume4                    0          0
C:
\Device\HarddiskVolume7                    0          0
D:
"#;

        let parsed = parse_fltmc_volume_device(output, &[String::from("D:")]);
        assert_eq!(parsed.as_deref(), Some(r"\Device\HarddiskVolume7"));
    }

    #[test]
    fn parses_native_volume_device_when_fltmc_uses_trailing_backslash_aliases() {
        let output = r#"
Filter Manager volumes
----------------------
Volume Name                                Altitude   Frame
-----------------------------------------  --------   -----
\Device\HarddiskVolume9                    0          0
D:\
"#;

        let parsed = parse_fltmc_volume_device(output, &[String::from("D:")]);
        assert_eq!(parsed.as_deref(), Some(r"\Device\HarddiskVolume9"));
    }

    #[test]
    fn candidate_bcd_volume_specs_always_include_drive_letter_fallback() {
        let candidates = candidate_bcd_volume_specs("D:");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.partition_device == "partition=D:"),
            "drive-letter partition fallback should always be present"
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.ramdisk_prefix == "[D:]"),
            "drive-letter ramdisk prefix fallback should always be present"
        );
    }
}
