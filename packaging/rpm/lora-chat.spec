Name:           lora-chat
Version:        0.1.0
Release:        1%{?dist}
Summary:        Privacy-first Linux TUI LoRa chat for RAK4631 / WisBlock
License:        MIT
BuildRequires:  rust, cargo

%description
Terminal chat over LoRa for RAK4631 WisBlock hardware with Meshtastic and
RAK RUI3 firmware support.

%prep

%build
cargo build --release

%install
install -Dm755 target/release/lora-chat %{buildroot}%{_bindir}/lora-chat

%files
%{_bindir}/lora-chat
