# Release process

Release preparation happens in a normal pull request. Update the version in
`Cargo.toml`, refresh `Cargo.lock`, move the completed notes from `Unreleased`
into a versioned `CHANGELOG.md` section, and merge only after CI passes.

To rehearse a release, open the `Release` workflow in GitHub Actions, select
`main`, leave the tag as `dry-run`, and run the workflow. To publish, run the
same workflow from `main` with a tag matching the package version, such as
`v0.1.0`. The workflow creates the tag and GitHub Release only after all target
artifacts build successfully.
