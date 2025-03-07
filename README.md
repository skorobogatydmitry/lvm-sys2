# lvm-sys2
Basic FFI lvm bindings for Rust.
This crate, unlike the original lvm-sys, uses the new header `lvm2cmd.h` instead of `lvm2app.h`, which is not available starting from around 2018.

This library was developed on
```text
  LVM version:     2.03.30(2) (2025-01-14)
  Library version: 1.02.204 (2025-01-14)
  Driver version:  4.49.0
```

# Usage

Library contains unsafe bindings for `lvm2cmd.h` and a safe wrapper in [lvm::Lvm].

# Non-root execution
To run LVM commands as non-root (e.g. in crate's tests), you need to
1. Temporary (until next reboot)
  - `sudo usermod -aG disk USERNAME` and re-login
  - `sudo chmod g+rw /dev/mapper/control; sudo chown :disk /dev/mapper/control`
  - `sudo chmod g+rwx /run/lock/lvm; sudo chown :disk /run/lock/lvm`
  - `sudo chmod g+rw /run/lvm/hints; sudo chown :disk /run/lvm/hints`
  - `sudo find /run/lvm/ -type f -print -exec chmod g+rw {} \;`
  - `sudo find /run/lvm/ -type s -print -exec chmod g+rw {} \;`
  - `sudo find /run/lvm/ -type d -print -exec chmod g+rwx {} \;`
  - `sudo find /run/lvm/ -type f -print -exec chown :disk {} \;`
  - `sudo find /run/lvm/ -type s -print -exec chown :disk {} \;`
  - `sudo find /run/lvm/ -type d -print -exec chown :disk {} \;`
  - `sudo setcap cap_sys_admin,cap_fowner+ep BINARY_NAME`

Refreshing:
```bash
BIN_NAME=`which lvm`
sudo chmod g+rw /dev/mapper/control; sudo chown :disk /dev/mapper/control
sudo chmod g+rwx /run/lock/lvm; sudo chown :disk /run/lock/lvm
sudo chmod g+rw /run/lvm/hints; sudo chown :disk /run/lvm/hints
sudo find /run/lvm/ -type f -print -exec chmod g+rw {} \;
sudo find /run/lvm/ -type s -print -exec chmod g+rw {} \;
sudo find /run/lvm/ -type d -print -exec chmod g+rwx {} \;
sudo find /run/lvm/ -type f -print -exec chown :disk {} \;
sudo find /run/lvm/ -type s -print -exec chown :disk {} \;
sudo find /run/lvm/ -type d -print -exec chown :disk {} \;
sudo setcap cap_sys_admin,cap_fowner+ep $BIN_NAME
```

1. Permanent: **TBD**

> Quick research for `/dev/mapper/control` showed that it, it seems, appears before any udev rules take effect.

# Docs
`cargo doc --open --document-private-items`

# Etc

## LVM DBus interface

Command to list services:

```bash
dbus-send --session           \
  --dest=org.freedesktop.DBus \
  --type=method_call          \
  --print-reply               \
  /org/freedesktop/DBus       \
  org.freedesktop.DBus.ListNames
```

... doesn't show LVM.

There's no DBus daemon on Manjaro => [raised a question on forum](https://forum.manjaro.org/t/no-lvmdbusd-binary-in-lvm2-package)

Command to check methods in the future:
```bash
dbus-send --system --type=method_call --print-reply \
      --dest=org.asamk.Signal \
      /org/asamk/Signal \
      org.freedesktop.DBus.Introspectable.Introspect
```