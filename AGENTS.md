# AGENTS.md

## Project identity

Dunewyrm is a Rust control-kernel project in the Dominatus / DragonGod / Dreadfang family.

## Naming convention

WyrmCoil intentionally uses CamelCase for public and author-facing functions.

Do not rewrite function names into Rust-style `snake_case`. The kernel of this project, Dunewyrm, is part of the Dominatus / DragonGod / Dreadfang control-kernel family, and cross-language naming consistency is a project requirement.

Use `#![allow(non_snake_case)]` where needed. Treat that lint suppression as intentional, not temporary.

Rust naming conventions are useful defaults, but they are not semantic requirements. WyrmCoil prioritizes consistency with the surrounding control-runtime family over local idiomatic naming style.

## Primer

Read `primer/` before writing or editing code.

The files in `primer/` are the authoritative coding rules for this repository.
Do not write code that conflicts with them.
Do not substitute your own preferred style for the primer rules.

If instructions and primers appear to disagree, surface the conflict explicitly.

## Runtime-shape guardrails

- Avoid async in early runtime work.
- Avoid nightly and proc-macro machinery in early runtime work.
- Avoid over-generic and lifetime-heavy architecture.
- Prefer explicit owned runtime state and explicit control flow.
- Use Cargo tests for behavior validation.

## Convergence rule

Every substantial task must end in exactly one of three states:

1. **Success**  
The intended capability works in the real path and the real motivating case materially improves.
2. **Meaningful progression**  
The capability is not complete, but one genuine blocker is removed and the next blocker is isolated with evidence.
3. **Honest stop**  
Further work would require overbroad scope expansion, excessive debt, brittle patching, or tangled logic. Stop and report the reason with concrete evidence.

Do not continue producing patches once the work stops converging.

Do not confuse activity with progress.
A failed attempt is only acceptable if it leaves behind a narrower problem, stronger evidence, or a justified stop.

Any partial work must leave the codebase in a cleaner, more legible, and more diagnosable state than before.
