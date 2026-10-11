<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plans

One file per spec, written by `uas-planner` (see `.claude/agents/README.md`, "Build handoff").
Name: `<NNNN>-<slug>.md`, the spec file stem. A plan lists packages, each with files, a
done-when command from the spec, the red-first control, the persona, and a wave number.
The integrator dispatches one wave at a time, at most three builders, and deletes or
marks a plan done when every package has landed.
