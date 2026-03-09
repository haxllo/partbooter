# Boot Flow

## Shared rules

- v1 is `UEFI + GPT` only
- existing ESP only
- all PartBooter-managed artifacts live under `EFI/PartBooter`
- all boot entry changes are additive
- verification must complete before the operation is reported as successful

## Linux ISO flow

### Supported input

- profiled Linux ISO names that map to an implemented handler

### Planning behavior

1. Detect profile support from the ISO path
2. Mark unknown profiles as `planned-only`
3. Build a plan that backs up the managed ESP area and current boot metadata
4. Stage extracted boot artifacts rather than relying on generic loopback
5. Register a managed additive boot entry
6. Verify the staged artifacts and entry presence

### Blocked behavior

- unknown distro ISO applies
- generic “boot any ISO” promises
- replacing the Windows default path

## WinPE flow

### Supported input

- `boot.wim`
- explicitly named WinPE WIM payloads

### Planning behavior

1. Detect WIM-based WinPE input
2. Reject raw Windows installer flows for v1 apply
3. Back up managed state before staging
4. Stage the WinPE-backed artifacts in the managed location
5. Register a managed additive Windows boot entry
6. Verify the entry and staged files

### Blocked behavior

- arbitrary Windows ISO apply
- repartitioning to create a new recovery layout

## Rollback principle

Rollback restores the managed PartBooter state and managed entry references. It
does not attempt to rewrite the machine into a new default boot configuration.

