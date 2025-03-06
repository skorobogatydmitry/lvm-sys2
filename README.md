# lvm-sys2
Basic FFI lvm bindings for Rust.
This crate, unlike the original lvm-sys, uses the new header `lvm2cmd.h` instead of `lvm2app.h`, which is not available starting from around 2018.

# Non-root running
To run apps as non-root (e.g. tests), you need to
1. Temporary (until next reboot)
  - `sudo usermod -aG disk USERNAME` and re-login
  - `sudo chmod g+rw /dev/mapper/control; sudo chown :disk /dev/mapper/control`
  - `sudo chmod g+rw /run/lock/lvm; sudo chown :disk /run/lock/lvm`
  - `sudo chmod g+rw /run/lvm/hints; sudo chown :disk /run/lvm/hints`
  - `sudo find /run/lvm/ -type f -print -exec chmod g+rw {} \;`
  - `sudo find /run/lvm/ -type s -print -exec chmod g+rw {} \;`
  - `sudo find /run/lvm/ -type d -print -exec chmod g+rwx {} \;`
  - `sudo find /run/lvm/ -type f -print -exec chown :disk {} \;`
  - `sudo find /run/lvm/ -type s -print -exec chown :disk {} \;`
  - `sudo find /run/lvm/ -type d -print -exec chown :disk {} \;`
  - `sudo setcap cap_sys_admin,cap_fowner+ep BINARY_NAME` - optional
    ```
    device-mapper: version ioctl on   failed: Permission denied
    Incompatible libdevmapper 1.02.204 (2025-01-14) and kernel driver (unknown version).
    ```
2. Permanent: **TBD**

# Docs
`cargo doc --open --document-private-items`

# LVM DBus interface

Command to list services:

```
dbus-send --session           \
  --dest=org.freedesktop.DBus \
  --type=method_call          \
  --print-reply               \
  /org/freedesktop/DBus       \
  org.freedesktop.DBus.ListNames
```

... doens't show LVM.

There's no DBus daemon on Manjaro => https://forum.manjaro.org/t/no-lvmdbusd-binary-in-lvm2-package

Command to check methods in the future:
```
dbus-send --system --type=method_call --print-reply \
      --dest=org.asamk.Signal \
      /org/asamk/Signal \
      org.freedesktop.DBus.Introspectable.Introspect
```