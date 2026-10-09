# Contributing

How this repository is maintained:

- Development happens in a private GitLab repository, which is the source of truth.
  The public GitHub repository is a mirror of its app code.
- After pushing to GitLab `main`, a maintainer runs the mirror script. It
  publishes GitLab `main` minus the private paths as one sync commit on GitHub `main`
  (never a force-push), after checking the exported tree for private material.
- Releases are built on GitHub from the mirrored tree.
- Changes proposed on GitHub are ported into GitLab first and arrive back here with
  the next sync; the script refuses to overwrite a GitHub change that has not been ported.

Engineering docs: `docs/architecture.md` (start here), `docs/commands.md`, `docs/tools.md`,
`MODELS.md`, and `apps/mcp/README.md`.
