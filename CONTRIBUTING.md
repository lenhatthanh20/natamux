# Contributing

Thank you for helping. Read [docs/dev-setup.md](docs/dev-setup.md) for the setup and the checks, and [AGENTS.md](AGENTS.md) for the code rules. Builds and checks run in CI or a codespace, not on a local machine.

## Rules for every contribution

- **MIT in, MIT out.** Your contribution is under the same licence as Natamux, [MIT](LICENSE). By opening a pull request you agree to this. You keep your copyright.
- **Sign off each commit** with `git commit -s`. The `Signed-off-by: Your Name <you@example.com>` line says you accept the [Developer Certificate of Origin 1.1](https://developercertificate.org/): you wrote the change, or you have the right to give it under MIT.
- **No copied code from other licences.** Do not copy code, text or structure from GPL or AGPL projects, from code with no licence or an unknown licence, or from your employer's code or products. Other tools may give ideas, never code.
- **You answer for AI-assisted code.** If an agent wrote part of your change, you are the author. Read it, test it, and check that it copies nothing that breaks the rules above.

## Dependencies

Ask before you add a crate or a package. A new source must be allowed in `deny.toml`, and its licence must be one of the ids in `deny.toml`. See [For a security review](https://natamux.github.io/docs/help/security-review/#how-the-dependencies-are-controlled).

## Security problems

Do not open a public issue. Use the private report in [SECURITY.md](SECURITY.md).
