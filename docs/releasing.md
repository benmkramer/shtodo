# Release process

Release preparation happens in a normal pull request. Update the version in
`Cargo.toml`, refresh `Cargo.lock`, move the completed notes from `Unreleased`
into a versioned `CHANGELOG.md` section, and merge only after CI passes.

To rehearse a release, open the `Release` workflow in GitHub Actions, select
`main`, leave the tag as `dry-run`, and run the workflow. To publish, run the
same workflow from `main` with a tag matching the package version, such as
`v0.1.0`. The workflow creates the tag and GitHub Release only after all target
artifacts build successfully.

## Homebrew publishing

The release workflow generates a Homebrew formula from the release archives
and publishes it to `benmkramer/homebrew-tap`. The formula pins the release
URLs and SHA-256 checksums for Apple Silicon macOS, Intel macOS, and x86-64
Linux. Publishing requires a repository secret on `benmkramer/shtodo`:

1. Create a fine-grained GitHub personal access token owned by `benmkramer`,
   restricted to `homebrew-tap`, with **Contents: Read and write** permission.
2. Add it as `HOMEBREW_TAP_TOKEN` in the shtodo repository's Actions secrets,
   or run the following command and paste the token at its private prompt:

   ```sh
   gh secret set HOMEBREW_TAP_TOKEN --repo benmkramer/shtodo
   ```

After a successful release, users can install with:

```sh
brew install benmkramer/tap/shtodo
```

Subsequent releases update the formula automatically. Users receive those
updates with `brew update` followed by `brew upgrade shtodo`.

`publish-prereleases = true` currently includes beta releases in the tap.
Set it to `false` when stable releases begin so later betas do not replace
the stable formula. A `dry-run` dispatch generates artifacts without
publishing the tap. Adding this configuration does not backfill older
releases; publish a new version or seed the tap with a verified formula
for an existing release.

After changing `dist-workspace.toml`, regenerate and check the workflow with
the pinned cargo-dist version:

```sh
dist generate --mode ci
dist generate --mode ci --check
dist plan
```

See the [cargo-dist Homebrew guide] for the publishing configuration and the
[GitHub token guide] for credential setup.

[cargo-dist Homebrew guide]: https://axodotdev.github.io/cargo-dist/book/installers/homebrew.html
[GitHub token guide]: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens
