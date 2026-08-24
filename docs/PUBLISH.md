# Publishing and Distribution Guide for nxvim

This guide outlines how `nxvim` is packaged, how to publish updates manually to various package registries, and how to automate releases.

---

## 1. Package Architectures & Files Created

The workspace contains the following files configures for building and packaging:

* **Cargo Packaging Integration**: Configured inside [`Cargo.toml`](file:///home/iceman/Developer/rust/nextvim/nxvim/Cargo.toml):
  * `[package.metadata.deb]`: Metadata configuration for building Debian (`.deb`) packages.
  * `[package.metadata.generate-rpm]`: Metadata configuration for building RedHat (`.rpm`) packages.
* **Arch Linux (AUR)**: [`packaging/PKGBUILD`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/PKGBUILD) defines the installation and packaging rules for Arch Linux.
* **RedHat / Fedora (Spec)**: [`packaging/nxvim.spec`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/nxvim.spec) defines RPM-packaging parameters.
* **General Install**: [`Makefile`](file:///home/iceman/Developer/rust/nextvim/nxvim/Makefile) provides standard `make && make install` targets for generic source distributions.
* **Automated local builds**: [`packaging/build-packages.sh`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/build-packages.sh) compiles the project and generates `.deb` and `.rpm` packages locally.

---

## 2. How to Publish Packages Manually

### Arch Linux (AUR / `yay`)
To publish or update the `nxvim` package on the Arch User Repository:
1. Register/login at [aur.archlinux.org](https://aur.archlinux.org/).
2. Clone your package repository:
   ```bash
   git clone ssh://aur@aur.archlinux.org/nxvim.git
   cd nxvim
   ```
3. Copy the [`packaging/PKGBUILD`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/PKGBUILD) to this repo.
4. Generate/update sums and metadata:
   ```bash
   updpkgsums
   makepkg --printsrcinfo > .SRCINFO
   ```
5. Commit and push:
   ```bash
   git add PKGBUILD .SRCINFO
   git commit -m "Release v0.1.0"
   git push origin master
   ```

### Fedora COPR (`dnf` / `rpm`)
For Fedora/CentOS/RHEL package hosting:
1. Create an account on [copr.fedorainfracloud.org](https://copr.fedorainfracloud.org/).
2. Set up a project pointing to your Github repo (using the spec file [`packaging/nxvim.spec`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/nxvim.spec)).
3. Alternatively, upload a source RPM built locally via:
   ```bash
   rpmbuild -bs packaging/nxvim.spec
   ```

### Rust Ecosystem (`cargo install`)
To publish onto `crates.io`:
1. Log in using your API token:
   ```bash
   cargo login <api-token>
   ```
2. Enable publishing in [`Cargo.toml`](file:///home/iceman/Developer/rust/nextvim/nxvim/Cargo.toml) by changing `publish = false` to `publish = true` (or deleting the line).
3. Publish:
   ```bash
   cargo publish
   ```

---

## 3. Releasing a New Version

Use this checklist for each release. The Fedora and Arch recipes are pinned to a source commit because the repository vendors both `syntect` and the Cargo registry dependencies for offline builds.

### 3.1 Update and test the source

1. Choose the next version according to the project versioning policy. For example, use `0.1.1` for a bug-fix release or `0.2.0` for a feature release.
2. Update the root package version in [`Cargo.toml`](file:///home/iceman/Developer/rust/nextvim/nxvim/Cargo.toml):
   ```toml
   version = "0.2.0"
   ```
3. Regenerate the lockfile only when dependencies or package metadata require it. Test without network access:
   ```bash
   cargo check --workspace --offline
   cargo check --workspace --locked --offline
   cargo build --release --locked --offline
   ```
4. If dependencies changed, refresh the vendor tree and Cargo configuration:
   ```bash
   cargo vendor vendor > .cargo/config.toml
   cargo check --workspace --locked --offline
   ```
   Do not ignore `vendor/cc/src/target/`: that directory contains real source files even though the directory name resembles a build directory.

### 3.2 Update package metadata

Update both package recipes:

* In [`packaging/nxvim.spec`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/nxvim.spec), update `Version`, reset `Release` to `1`, and add the release notes to `%changelog`.
* In [`packaging/PKGBUILD`](file:///home/iceman/Developer/rust/nextvim/nxvim/packaging/PKGBUILD), update `pkgver` and reset `pkgrel` to `1`.

Commit and push the complete source changes first, without the new source pin:

```bash
git add Cargo.toml Cargo.lock .cargo/config.toml vendor crates packaging
# Verify that unrelated files and generated artifacts are not staged.
git status --short
git commit -m "Prepare nxvim 0.2.0"
git push remote main
```

Get the resulting full commit hash:

```bash
git rev-parse HEAD
```

Set that hash in both recipes:

```spec
# packaging/nxvim.spec
%global commit <full-commit-hash>
```

```bash
# packaging/PKGBUILD
_commit=<full-commit-hash>
```

For the Arch package, calculate and record the source checksum:

```bash
curl -L --fail \
  "https://github.com/nextvim/nxvim/archive/<full-commit-hash>.tar.gz" \
  -o nxvim-source.tar.gz
sha256sum nxvim-source.tar.gz
```

Replace the `sha256sums` value in `PKGBUILD` with the result and remove the downloaded archive.

### 3.3 Validate and publish

Validate the recipes before publishing:

```bash
rpmspec -P packaging/nxvim.spec >/dev/null
bash -n packaging/PKGBUILD
cargo build --release --locked --offline
```

Commit and push the recipe update:

```bash
git add packaging/nxvim.spec packaging/PKGBUILD
git commit -m "Update package recipes for nxvim 0.2.0"
git push remote main
```

Create a release tag after the source is published:

```bash
git tag -a v0.2.0 -m "nxvim 0.2.0"
git push remote v0.2.0
```

The two source/recipe commits are intentional: the recipes point to the earlier commit containing the complete release source, avoiding a circular dependency on the commit hash embedded in the recipes.

### 3.4 Submit the Fedora COPR build

Use the raw spec URL, not a GitHub `blob` URL:

```text
https://raw.githubusercontent.com/nextvim/nxvim/main/packaging/nxvim.spec
```

Alternatively configure COPR as an SCM build:

* Repository: `https://github.com/nextvim/nxvim.git`
* Branch: `main`
* Spec file: `packaging/nxvim.spec`

Build Fedora 43 and Fedora 44 initially. The spec uses `cargo build --release --locked --offline`, and the pinned source archive must contain both `vendor/proptest/Cargo.toml` and `vendor/cc/src/target/parser.rs`. Ensure COPR is building the current `main` revision rather than a cached older source.

### 3.5 Test the published package

After COPR succeeds, enable the repository and install the package on Fedora:

```bash
sudo dnf install 'dnf-command(copr)'
sudo dnf copr enable icedman/nxvim
sudo dnf clean all
sudo dnf makecache --refresh
sudo dnf install nxvim
```

Verify the package and executable:

```bash
rpm -q nxvim
dnf info nxvim
rpm -ql nxvim
rpm -V nxvim
nxvim
```

`rpm -V nxvim` should produce no output when the installed files match the package. For a clean test, repeat the installation in a Fedora 43 or Fedora 44 container.

---

## 4. Automating Releases (CI/CD)

To fully automate the release process, you can use the following GitHub Actions workflow. When you push a git tag starting with `v` (e.g., `git tag v0.1.0 && git push --tags`), it compiles the binary, packages it into `.deb` and `.rpm` packages, and creates a GitHub Release.

Save this configuration to `.github/workflows/release.yml`:

```yaml
name: Publish Release

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust Toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install Cargo Packaging Tools
        run: cargo install cargo-deb cargo-generate-rpm

      - name: Build release binary
        run: cargo build --release

      - name: Package .deb & .rpm
        run: |
          cargo deb
          cargo generate-rpm

      - name: Upload Binaries and Packages to GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          files: |
            target/release/nxvim
            target/debian/*.deb
            target/generate-rpm/*.rpm
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```
