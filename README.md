# PartBooter

PartBooter is a Windows-first boot orchestration tool for preparing supported
internal boot payloads without relying on removable USB media.

This repository currently contains:

- a Rust workspace scaffold for the CLI, helper, and core crates
- shared domain types for probe, plan, apply, verify, rollback, and repair
- a journal/checkpoint model for apply, verify, rollback, and repair
- architecture and product documents for the v1 scope

## Current scope

The scaffold is intentionally conservative:

- Windows host workflows only
- UEFI and GPT only
- supported Linux ISO profiles and WinPE payloads only
- additive boot-entry model only
- no repartitioning and no BIOS/MBR support

## Probe behavior

`partbooter probe` now performs a live Windows host inspection via native
PowerShell/Storage cmdlets when run on Windows. It collects:

- firmware mode
- system disk partition style
- Secure Boot state
- BitLocker presence
- EFI System Partition details

Non-Windows hosts fail fast because live probing is Windows-only.

## Commands

```text
partbooter probe --json
partbooter plan --payload <path> --target <volume> --json
partbooter plan --payload <path> --target <volume> --out sample.pbplan
partbooter apply --plan sample.pbplan
partbooter verify --operation <operation-id>
partbooter rollback --operation <operation-id>
partbooter repair --latest
```

The helper binary mirrors the privileged lifecycle surface and is the future
transport boundary for Windows elevation/service integration.

`apply`, `verify`, `rollback`, and `repair` still operate as scaffolded journal
and plan lifecycle commands. Real ESP/BCD mutation remains a later milestone.

## Build

```bash
cargo test
```
