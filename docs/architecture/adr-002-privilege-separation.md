# ADR-002: Use a Split CLI and Elevated Helper Model

## Status

Accepted

## Context

PartBooter must inspect systems freely but only mutate boot state through a
narrow trusted path. A single always-elevated process would be simpler, but it
would blur the trust boundary and make a future desktop product harder to build.

## Decision drivers

- reduce the amount of code that runs with elevated privileges
- keep the operator surface usable without permanent elevation
- make rollback and repair flows explicit and auditable
- preserve a path to service-backed or desktop-backed transports later

## Considered options

### Split CLI + elevated helper

Pros:

- clear privilege boundary
- easier to audit and harden
- future UI can reuse the same helper contract

Cons:

- more moving parts than a single process

### Single elevated CLI

Pros:

- fastest first implementation

Cons:

- too much logic runs with elevated rights
- poor fit for future desktop packaging

## Decision

Use an unprivileged CLI for planning and a separate elevated helper for
write-bearing actions.

## Consequences

Positive:

- safer operational boundary
- clearer separation between planning and execution
- easier long-term scalability for additional clients

Negative:

- transport and helper lifecycle need deliberate design
- helper testing must cover partial-failure and checkpoint scenarios

