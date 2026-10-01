# Publishing Kova Native

The workspace contains ten publishable library packages, all at 0.2.0. All
use the SPDX identifier `MPL-2.0`, Rust 2024 and a declared minimum Rust version
of 1.89. `cosmic-text` 0.19 requires Rust 1.89. The publishing workflow below
uses the workspace packaging/publishing support of Cargo 1.98.1.

## Package names and imports

| Package / directory under `crates/` | Rust crate name |
| --- | --- |
| kova-native | kova_native |
| kova-native-core | kova_native_core |
| kova-native-animation | kova_native_animation |
| kova-native-assets | kova_native_assets |
| kova-native-input | kova_native_input |
| kova-native-layout | kova_native_layout |
| kova-native-platform | kova_native_platform |
| kova-native-text | kova_native_text |
| kova-native-render | kova_native_render |
| kova-native-widgets | kova_native_widgets |

The names, paths, dependencies, Rust imports and examples all use the new prefix.
There are no legacy dependency aliases or library-name overrides. Consumers use:

```toml
[dependencies]
kova-native = "0.2.0"
```

```rust
use kova_native::prelude::*;
```

Internal dependencies keep both `path` and `version`: the path is used while
developing the workspace, and Cargo's normalized registry manifests retain the
version requirement instead. Change the shared package version and the internal
dependency versions together when releasing the subsystems. The facade can
receive its own compatible patch without republishing unchanged subsystem
libraries; update its package version and workspace dependency version together.

Libraries share their README, license, keywords, categories, repository, MSRV,
registry restriction and inclusion list through workspace metadata.
Each has its own docs.rs URL and a copy of the root MPL-2.0 `LICENSE`. Keep those
license copies identical to the root file. Archives include source files,
including WGSL shaders, README and LICENSE; examples remain `publish = false`.

## Validate packages

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo package --workspace --exclude animation --exclude buttons --exclude hello_window --exclude layout --exclude showcase --allow-dirty
cargo publish --workspace --dry-run --allow-dirty
```

`cargo package` creates `.crate` archives in `target/package` and compiles the
extracted packages using Cargo's temporary registry for the workspace dependencies.
This checks that they build without workspace-only paths. The dry run performs
publication checks without uploading. `--allow-dirty` is useful while preparing
the release; commit the final changes and omit this flag for the real release.

## Publish the release

After reviewing the final source, license and package contents, authenticate
locally with crates.io and run:

```powershell
cargo login
cargo publish --workspace
```

Never commit a registry token. `publish = ["crates-io"]` restricts the libraries
to crates.io, and the unpublished examples are skipped. Cargo publishes the
selected library packages in dependency order.

For a release that hits the new-package rate limit, or a partially completed
upload, use the resumable PowerShell helper instead:

```powershell
.\scripts\publish-crates.ps1 -Plan
.\scripts\publish-crates.ps1
```

The plan only reads Cargo metadata and registry versions. The publication helper
discovers the dependency order, skips versions already present, uses `cargo
publish -p` with verification, and waits until the retry time returned by a 429
response. It stops on other failures or if the release checkout changes. The
default total time limit is 65 minutes; `-MaxMinutes` can extend it. Commit changes
before running it. Keep the checkout unchanged while it is publishing.

Crates.io allows an initial burst of five new packages, then one additional new
package every ten minutes. A fresh ten-package workspace release can therefore
take about fifty minutes after the initial burst. See the
[registry rate limits](https://crates.io/docs/rate-limits).

For publishing one package at a time, use this valid dependency order:

1. kova-native-core
2. kova-native-animation
3. kova-native-assets
4. kova-native-input
5. kova-native-layout
6. kova-native-platform
7. kova-native-text
8. kova-native-render
9. kova-native-widgets
10. kova-native

Use `cargo publish -p <package>` and wait for each dependency to become available
in the registry before continuing. The old names `kova` and `kova-core` were
already owned by other accounts when this release was prepared. New-name
availability and account authorization are ultimately checked by crates.io at
upload time.

Reference: [Cargo packaging](https://doc.rust-lang.org/cargo/commands/cargo-package.html),
[Cargo publishing](https://doc.rust-lang.org/cargo/commands/cargo-publish.html),
[Mozilla's MPL-2.0 text](https://www.mozilla.org/MPL/2.0/).
