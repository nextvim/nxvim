Name:           nxvim
Version:        0.1.0
Release:        1%{?dist}
Summary:        A Vim-inspired terminal text editor written in Rust

License:        GPL-3.0-or-later AND Apache-2.0
URL:            https://github.com/user/nxvim
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust

%description
A Vim-inspired terminal text editor written in Rust, powered by Zed's ultra-high-performance Rope + SumTree-backed text buffers and concurrent snapshot technologies.

%prep
%autosetup

%build
cargo build --release --locked

%install
rm -rf %{buildroot}
install -D -p -m 755 target/release/nxvim %{buildroot}%{_bindir}/nxvim
install -D -p -m 644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%license LICENSE
%{_bindir}/nxvim

%changelog
* Sat Aug 22 2026 Maintainer <maintainer@example.com> - 0.1.0-1
- Initial package release
