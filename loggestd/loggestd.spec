Name: loggestd
Summary: The loggest log daemon
Version: %{_VERSION}
License: ASL 2.0
Release: 1
BuildRequires: systemd

%description
%{summary}.

%build
cargo build --release --target %{_TARGET} %{?_CARGO_BUILD_ARGS}

%install
install -D -m 755 %{_sourcedir}/target/%{_TARGET}/release/%{name} -t %{buildroot}%{_bindir}
install -D -m 755 %{_sourcedir}/loggestd.service -t %{buildroot}%{_unitdir}

%post
systemctl enable %{name}.service
systemctl start %{name}.service

%preun
if [ $1 -eq 0 ]; then
    systemctl disable %{name}.service
    systemctl stop %{name}.service
fi


%files
%{_bindir}/*
%{_unitdir}/*
