# Release

`xabi` publishes crates with crates.io Trusted Publishing from GitHub Actions.

## Trusted Publisher Configuration

Configure trusted publishers on crates.io for all published crates:

- `xabi-macros`
- `xabi`
- `xabi-assert`

Use the following GitHub Actions publisher identity for each crate:

- Repository: `Xuanwo/xabi`
- Workflow: `release.yml`
- Environment: `crates-io`

No long-lived crates.io API token is required in GitHub secrets.

## Publishing

Create and publish a GitHub Release whose tag matches the crate version:

```sh
v0.1.0-alpha.3
```

The release workflow verifies that:

- the `xabi-macros`, `xabi`, and `xabi-assert` crate versions match;
- the release tag is `v<crate-version>`.

The workflow uses Cargo workspace publishing:

```sh
cargo publish --workspace --locked
```

Cargo selects only publishable workspace members and publishes them in
dependency order.
