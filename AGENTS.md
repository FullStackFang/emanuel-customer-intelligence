# Project Agent Instructions

## Specification Workflow

- Use OpenSpec as the default workflow for new features, behavior changes, architectural changes, and any user request for a specification.
- Invoke the `openspec-propose` skill and create the change under `openspec/changes/<change-name>/`.
- Treat the OpenSpec `proposal.md`, capability specs, `design.md`, and `tasks.md` as the authoritative implementation contract.
- Generate each artifact from `openspec instructions <artifact> --change <change-name> --json`; do not invent an alternate spec structure.
- Validate the change with the repository-local OpenSpec CLI before implementation.
- Do not add new feature specifications under `docs/superpowers/specs/`. Existing files there are historical unless an OpenSpec change explicitly references them.
- Small isolated bug fixes do not require a full OpenSpec change unless the user requests a spec or the fix reveals architectural scope.
- After implementation and verification, use the OpenSpec archive workflow to promote capability specs and archive the completed change.

Run the local CLI through `npm exec -- openspec <command>`.
