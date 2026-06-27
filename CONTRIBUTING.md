# Contributing to Rigger

Thanks for your interest in Rigger. Contributions are welcome, and they go through a controlled pull-request process so the project stays coherent.

## How contributions land

Rigger uses a fork-and-pull model with required review. `main` is protected: every change lands through a reviewed, CI-green pull request, and direct pushes to `main` are not allowed.

1. Fork the repository and create a branch off `main`.
2. Make your change. Keep it focused: one logical change per pull request.
3. Run `go build ./...`, `go vet ./...`, and `go test ./...` and make sure they pass.
4. Open a pull request against `main`. CI runs automatically.
5. A maintainer reviews. Once it is approved and CI is green, it can be merged.

## Ground rules

- Match the architecture. `docs/architecture.md` is the source of truth for how Rigger is built. If your change diverges from it, say so in the pull request and explain why. Large divergences should start as an issue or a discussion before you write code.
- Keep pull requests small and reviewable. A reviewer should be able to hold the whole change in their head at once.
- Write tests. New behavior comes with tests. A bug fix comes with a test that fails before the fix and passes after it.
- No unrelated changes. Formatting churn and drive-by refactors do not belong in the same pull request as a feature or fix.

## Sign-off

We use the Developer Certificate of Origin (DCO). Add a `Signed-off-by` line to each commit by committing with `git commit -s`. That line certifies you wrote the change, or otherwise have the right to submit it under the project license.

## Reporting bugs and proposing features

Open an issue using the templates. For anything security-related, do not open a public issue: see [SECURITY.md](SECURITY.md).

## License

By contributing, you agree that your contributions are licensed under the project's license (see [LICENSE](LICENSE)).
