# System Architecture

## Overview

PartBooter is a layered Rust workspace built around a single lifecycle:

`probe -> plan -> apply -> verify -> rollback -> repair`

The design separates orchestration from privilege so the operator-facing surface
stays stable while the elevated implementation evolves behind it.

## Components

### `apps/cli`

Operator-facing CLI. Responsible for argument parsing, invoking the core
library, and rendering human-readable or JSON output.

### `apps/helper`

Future elevated helper boundary. In v1 it exposes the privileged lifecycle shape
and keeps admin-only work separate from the CLI.

### `crates/core`

Owns orchestration logic:

- machine probe aggregation
- payload selection
- execution plan construction
- lifecycle command coordination

### `crates/common`

Shared domain model for:

- `MachineProbe`
- `PayloadSpec`
- `ExecutionPlan`
- `OperationJournal`
- `VerificationReport`

### `crates/windows`

Windows-specific adapter for:

- machine probing
- managed ESP path conventions
- backup targets
- verification targets

### `crates/payloads/*`

Payload handlers behind a shared model. Each handler decides whether a payload
is supported, planned-only, or unsupported, and emits staging guidance.

### `crates/journal`

Operation-state foundation. Generates stable operation identifiers, lifecycle
checkpoints, rollback stubs, and verification summaries.

## Control flow

### Probe

Collect host facts and determine whether the environment can proceed to apply.

### Plan

Resolve the payload handler, compute additive steps, attach risk flags, and
emit backup targets before any privileged change.

### Apply

The helper owns write-bearing steps:

- backup managed state
- stage managed artifacts
- register managed boot entry
- checkpoint progress

### Verify

Confirm the staged files, managed entry, and latest journal state.

### Rollback / repair

Restore the managed state from checkpoints without deleting or replacing the
default Windows boot path.

## Scalability rules

- Add new payloads by adding a crate or module, not by rewriting core planning
- Keep the CLI stable and let future UI layers call the same lifecycle surface
- Keep Windows-specific logic behind adapter functions
- Treat additive boot policy as an invariant, not a preference

