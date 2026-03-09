# ADR-001: Use Rust for the Core Runtime

## Status

Accepted

## Context

PartBooter modifies boot-related system state. The product needs:

- memory-safe systems code
- predictable binaries for Windows operators
- a strong module boundary between shared models and platform adapters
- room to grow into helper, service, or desktop-hosted deployments

## Decision drivers

- correctness is more important than raw development speed
- privileged code should avoid avoidable memory-safety bugs
- the codebase must stay portable enough for future tooling and test harnesses
- the workspace should compile cleanly without a large runtime stack

## Considered options

### Rust

Pros:

- strong memory-safety defaults
- good fit for CLI, helper, and systems libraries
- predictable workspace structure for layered crates

Cons:

- steeper learning curve than scripting languages

### C

Pros:

- maximum low-level control

Cons:

- poor default safety for privileged boot orchestration
- higher maintenance cost for correctness and testing

### C#

Pros:

- very productive for Windows-first tools

Cons:

- weaker fit for a future low-level helper/core split without pulling in a
  larger managed runtime surface

## Decision

Use Rust for the CLI, helper, core orchestration library, and shared model.

## Consequences

Positive:

- safer default implementation language for privileged workflows
- scalable workspace structure with crate-level boundaries
- future UI layers can stay thin and call into a stable core

Negative:

- more upfront design work for models and command boundaries
- contributors need Rust familiarity

