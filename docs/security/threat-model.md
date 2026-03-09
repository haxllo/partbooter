# Threat Model

## Protected assets

- existing Windows boot path
- EFI System Partition contents
- BCD and firmware boot-order state
- operator-selected payload paths
- operation journal and backup metadata

## Trust boundaries

### Unprivileged boundary

The CLI may inspect and plan, but it must not directly mutate boot state.

### Privileged boundary

The helper is the only component allowed to:

- write PartBooter-managed ESP artifacts
- modify boot entry configuration
- execute rollback and repair steps

## Primary threats

### Incomplete or unsafe host detection

Risk:

- attempting apply on unsupported firmware, partition layout, or encryption
  state

Control:

- block apply when required probe facts are missing or unsupported

### Corrupt or partial staged state

Risk:

- interrupted operations leave an inconsistent managed entry

Control:

- checkpoint each write-bearing phase
- require backup targets before apply
- keep rollback and repair as first-class commands

### Over-broad privilege

Risk:

- too much logic executes with elevated rights

Control:

- keep the helper narrow
- keep planning logic in the core/CLI path

### Unsupported payload execution

Risk:

- arbitrary ISO or installer media produces an unverified boot path

Control:

- handler-based allowlist
- `planned-only` classification for recognized but unsupported payloads

## Security posture for v1

- offline-first
- no network dependency for core lifecycle commands
- additive boot policy only
- no BIOS/MBR support
- no default boot-path replacement

