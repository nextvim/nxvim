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

## 3. Automating Releases (CI/CD)

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
