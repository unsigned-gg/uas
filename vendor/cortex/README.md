# Vendored Cortex Proto Schemas

Do not edit these files directly. Re-vendor them from the upstream repository.

- Source repository: `cerebral-work/cortex`
- Upstream path: `crates/cortex-domain/proto/cortex/mesh/v1/`
- Upstream commit: `a63d8588a5d27c98505f7e8822befebe78eaa8cb`
- Date vendored: 2026-10-08
- Licence: Proprietary (Cerebral)

## Role in CAS

These Protocol Buffer files define the Cortex Mesh v1 API.
They define messages and services for lanes, peers, pools, registries, teams, edge nodes, and admin commands.
CAS uses these schemas to generate mesh client interfaces and validate mesh messages without direct crate links.

## Contents

The directory `proto/` contains 7 Protocol Buffer schema definitions for Cortex Mesh v1.
