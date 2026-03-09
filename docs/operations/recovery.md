# Recovery and Rollback

## Operating principle

Recovery must restore the machine to the last known good managed state without
changing the operator’s established Windows boot path.

## Standard operator flow

1. Run `partbooter verify --operation <id>` after any apply
2. If verification fails, run `partbooter rollback --operation <id>`
3. If the latest operation state is unclear, run `partbooter repair --latest`

## Failure classes

### Planning blocked

Symptoms:

- payload unsupported
- host unsupported
- probe facts incomplete

Action:

- do not apply
- resolve the blocker or use a supported payload/profile

### Apply interrupted

Symptoms:

- helper stopped mid-run
- checkpoint artifacts exist but later steps did not complete

Action:

- inspect latest journal state
- restore the last checkpoint through rollback or repair

### Verification failed

Symptoms:

- managed entry missing
- backup manifest or BCD snapshot missing
- staged WinPE artifacts incomplete or removed

Action:

- run rollback for the affected operation
- re-run planning before another apply attempt

## Operator safeguards

- never delete the existing default Windows path as part of recovery
- prefer restoring the managed subset over broad boot reconfiguration
- keep all recovery decisions tied to a known operation identifier
