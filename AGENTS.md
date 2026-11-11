# AGENTS.md

See [CLAUDE.md](./CLAUDE.md) for build/run commands and high-level architecture.

Agent-facing documentation lives in [`docs/`](./docs/):

| File | Contents |
|------|----------|
| [docs/agent.md](./docs/agent.md) | Quick-reference: key files, commit rules, sharp edges |
| [docs/components.md](./docs/components.md) | How to add a component (`Part`), an island, a toolbox entry |
| [docs/ipc.md](./docs/ipc.md) | How to extend `IPCEvent` and IPC backends |
| [docs/data-sources.md](./docs/data-sources.md) | How to add a provider, or use one of AntiWebUI's |
| [docs/config.md](./docs/config.md) | `theme.toml` / `animation.toml`, `WatchableConfig` |

Start at **[docs/agent.md](./docs/agent.md)**.
