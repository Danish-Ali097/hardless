## What and why

<!-- What this changes and why. -->

## Requirements

<!-- FR/NFR IDs from docs/PRD.md this implements or affects, e.g. FR-06, FR-08. -->

## Definition of done (PRD section 15)

- [ ] `cargo fmt`, `clippy`, tests, and TS lint/typecheck pass locally and in CI on all three OSes
- [ ] Every FR touched has a test named after its ID
- [ ] New public APIs documented (rustdoc or TSDoc) and shown in the crate or package README
- [ ] Release note added (Conventional Commit for crates, `pnpm changeset` for npm packages)
- [ ] Dashboard changes work in light and dark themes (if any)
- [ ] Milestone checklist in PRD section 13 updated (if a milestone item is done)
- [ ] `cargo xtask gen-types` run if wire types changed
- [ ] New dependencies justified below (maintenance status, licence against NFR-14)

## Dependencies added

<!-- None, or name + licence + why. -->

## Deviations from the PRD

<!-- None, or what differs and why. Also log it in docs/decisions.md. -->
