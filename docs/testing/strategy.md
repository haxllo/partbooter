# Testing Strategy

## Test layers

### Unit tests

Cover:

- payload detection
- support classification
- plan construction
- risk-flag generation
- journal identifier stability

### Integration tests

Cover:

- CLI command surface
- helper command surface
- probe/plan/apply/verify/rollback/repair lifecycle wiring
- JSON output stability for automation consumers

### Windows adapter tests

Use controlled fixtures and mocks to validate:

- machine probe parsing
- backup target resolution
- verification target generation

### Hardware validation

Required before a real release:

- at least two Windows hardware vendors
- UEFI/GPT systems only
- one supported Linux ISO profile
- one WinPE WIM flow

## Release gates

- `cargo fmt --all`
- `cargo test`
- fixture-based lifecycle tests for supported and blocked payloads
- manual validation of rollback and repair playbooks

## Important negative cases

- non-Windows host
- non-profiled Linux ISO
- raw Windows ISO
- interrupted apply
- verification failure after staging

