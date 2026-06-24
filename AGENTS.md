# AGENTS.md

See [CLAUDE.md](./CLAUDE.md) for build/run commands, architecture, commit-message
conventions, and project status. It is the source of truth for working in this
repository.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/): write each
message as `<type>(<optional scope>): <description>`, lowercase description, no
trailing period. Common types here: `feat`, `fix`, `refactor`, `docs`, `test`,
`chore`. Use scopes that match the module being touched, e.g.
`feat(ipc): map niri workspace events`, `fix(ui): reset redraw flag`,
`refactor(font): cache typeface`.
