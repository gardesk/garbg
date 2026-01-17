Name:           garbg
Version:        0.1.0
Release:        1%{?dist}
Summary:        Wallpaper manager for the gar desktop suite

License:        MIT
URL:            https://github.com/gardesk/garbg
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  libxcb-devel

# Disable debug package
%global debug_package %{nil}

%description
Garbg is a wallpaper manager for X11 built in Rust. Supports PNG, JPEG, GIF,
and WebP formats with async loading. Part of the gardesk desktop environment
suite.

%prep
%autosetup

%build
export CARGO_TARGET_DIR=target
cargo build --release --workspace

%install
install -Dm755 target/release/garbg %{buildroot}%{_bindir}/garbg

%files
%{_bindir}/garbg

%changelog
* Fri Jan 17 2025 mfw <espadonne@outlook.com> - 0.1.0-1
- Initial RPM release of garbg
- Wallpaper manager with multiple format support
- Part of gardesk desktop suite
