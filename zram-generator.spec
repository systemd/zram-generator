%global debug_package %{nil}

%if 0%{?centos_version} == 700
%global _systemdgeneratordir %{_prefix}/lib/systemd/system-generators
%endif

%if 0%{?centos_version} == 700 || 0%{?centos_version} == 800 || 0%{?suse_version} > 1500 || 0%{?is_opensuse}
%global _systemd_util_dir %{_prefix}/lib/systemd
%endif

Name: zram-generator
Version: 0.3.2
Release: 100%{?dist}
Summary: Systemd unit generator for zram swap devices
License: MIT
URL: https://github.com/systemd/zram-generator
Source0: %{name}_%{version}.orig.tar.gz
BuildRequires: cargo

%description
This is a systemd unit generator that enables swap on zram. (With zram,
there is no physical swap device. Part of the avaialable RAM is used to
store compressed pages, essentially trading CPU cycles for memory.) To
activate, install zram-generator-defaults subpackage.

%package -n zram-generator-defaults
Summary: Default configuration for zram-generator
Requires: zram-generator = %{version}-%{release}
BuildArch: noarch

%description -n zram-generator-defaults
Activate zram-generator with default configuration.

%prep
%autosetup -T -c -n %{name}_%{version}-%{release}
tar -zx -f %{S:0} --strip-components=1 -C .

%build
make program systemd-service \
    SYSTEMD_UTIL_DIR=%{_systemd_util_dir} \
    SYSTEMD_SYSTEM_UNIT_DIR=%{_unitdir} \
    SYSTEMD_SYSTEM_GENERATOR_DIR=%{_systemdgeneratordir}

%install
install -Dpm755 -d %{buildroot}%{_systemdgeneratordir}
install -Dpm755 -d %{buildroot}%{_unitdir}
install -Dpm755 -d %{buildroot}%{_docdir}/zram-generator
install -Dpm755 -d %{buildroot}%{_prefix}/lib/systemd
install -Dpm755 target/release/zram-generator -t %{buildroot}%{_systemdgeneratordir}/
install -Dpm644 units/systemd-zram-setup@.service %{buildroot}%{_unitdir}/
install -Dpm644 zram-generator.conf.example %{buildroot}%{_docdir}/zram-generator/
install -Dpm644 usr/lib/systemd/zram-generator.conf -t %{buildroot}%{_prefix}/lib/systemd/

%files
%license LICENSE
%doc README.md
%{_docdir}/zram-generator/zram-generator.conf.example
%{_systemdgeneratordir}
%{_systemdgeneratordir}/zram-generator
%{_unitdir}/systemd-zram-setup@.service

%files -n zram-generator-defaults
%{_prefix}/lib/systemd/zram-generator.conf

%changelog
