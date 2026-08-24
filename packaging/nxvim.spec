Name:           nxvim
Version:        0.1.0
Release:        1%{?dist}
Summary:        A Vim-inspired terminal text editor written in Rust

%global commit ef671afa2a7834d6b83cf402cef5a6d5ce7f1bac

License:        VIM LICENSE, GPL-2.0-or-later
URL:            https://github.com/nextvim/nxvim
Source0:        https://github.com/nextvim/nxvim/archive/%{commit}/%{name}-%{commit}.tar.gz

# Rust/Cargo toolchain and native build dependencies used by the crate graph.
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  clang-devel
BuildRequires:  pkgconf-pkg-config

%description
A Vim-inspired terminal text editor written in Rust, powered by Zed's ultra-high-performance Rope + SumTree-backed text buffers and concurrent snapshot technologies.

%prep
%autosetup -n %{name}-%{commit}

%build
cargo build --release --locked --offline

%install
rm -rf %{buildroot}
install -D -p -m 755 target/release/nxvim %{buildroot}%{_bindir}/nxvim
install -D -p -m 644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%license LICENSE
%{_bindir}/nxvim

%changelog
* Sat Aug 22 2026 Maintainer <m4rvin2005@gmail.com> - 0.1.0-1
- Initial package release
