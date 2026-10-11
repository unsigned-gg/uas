<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# LeanSpec EARS Requirements Guide

This guide defines the Easy Approach to Requirements Syntax (EARS) for LeanSpec in this repository.

## What is EARS?

EARS provides a set of structured syntax patterns for natural language requirements.
It removes passive voice, hidden assumptions, and convoluted clauses.
LeanSpec files record functional and non-functional requirements under `## Design` in a `### Requirements (EARS)` subsection.

## The Five EARS Patterns

### 1. Ubiquitous Requirements
Active at all times. No trigger or operational state condition.
- **Pattern**: `The <system name> shall <system response>.`
- **Example**: `The uas-domain crate shall have zero external I/O dependencies.`

### 2. Event-Driven Requirements
Triggered when a specific event occurs.
- **Pattern**: `WHEN <trigger>, the <system name> shall <system response>.`
- **Example**: `WHEN a lease expires, the slot manager shall transition the slot to Draining.`

### 3. State-Driven Requirements
Active while the system is in a specific operational state.
- **Pattern**: `WHILE <state>, the <system name> shall <system response>.`
- **Example**: `WHILE a worker slot is frozen, the cgroup freezer controller shall stop CPU scheduling.`

### 4. Unwanted Behaviour / Exception Requirements
Triggered upon errors, invalid inputs, boundary violations, or failures.
- **Pattern**: `IF <condition / error trigger>, THEN the <system name> shall <system response>.`
- **Example**: `IF a worker process exits with an unhandled signal, THEN the engine shall invoke compensating actions.`

### 5. Optional Feature Requirements
Active only when a specific optional feature, hardware capability, or configuration is present.
- **Pattern**: `WHERE <feature is applicable>, the <system name> shall <system response>.`
- **Example**: `WHERE eBPF is supported by the kernel, uas-ebpfd shall attach sched_process_exec tracepoint probes.`

### 6. Complex / Compound Requirements
Combines an operational state or optional feature with a trigger or exception condition.
- **Pattern**: `WHILE <state>, WHEN <trigger>, the <system name> shall <system response>.`
- **Pattern**: `WHERE <feature>, IF <condition>, THEN the <system name> shall <system response>.`
