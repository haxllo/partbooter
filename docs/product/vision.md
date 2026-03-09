# PartBooter Vision

## Product statement

PartBooter is a Windows-first boot orchestration tool that prepares supported
Linux ISO and WinPE payloads on an internal disk so operators can boot them
without USB media. The product manages boot preparation as a controlled,
additive workflow rather than as a raw disk-writing utility.

## Target operator

- Power users
- IT administrators
- Lab and field-service operators

v1 is not aimed at casual consumers. The operator is expected to understand
basic Windows storage, UEFI, and recovery concepts.

## Core value

- Replace scattered manual boot-preparation steps with a single planned flow
- Preserve the existing Windows boot path
- Produce verifiable backups, checkpoints, and rollback guidance
- Keep the product extensible for additional payload handlers later

## v1 scope

- Windows host only
- `UEFI + GPT` only
- Existing ESP only
- Supported Linux ISO profiles only
- WinPE WIM-based flows only
- Additive boot entries only

## Explicit non-goals for v1

- BIOS or MBR support
- Repartitioning or shrinking disks
- Arbitrary ISO compatibility promises
- Replacing the default Windows boot path
- Network-backed workflows

## Success criteria

- A supported payload can be probed, planned, and represented as an additive
  execution plan without ambiguity
- The generated plan makes backup, staging, registration, and verification
  steps explicit
- Unsupported or risky environments fail early with clear blockers
- The CLI and helper surfaces remain stable enough for a future desktop client

