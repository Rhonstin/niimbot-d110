# Protocol attribution

The NIIMBOT packet framing, B1-family handshake, D110 one-page-per-job behavior,
and bitmap row encoding were implemented using the protocol documentation and
driver in [iscarelli/niimbot-web-bluetooth](https://github.com/iscarelli/niimbot-web-bluetooth).

Reference files: `docs/protocol-v4.md`, `src/niimbot.js`, `registry.json`.
Consulted on 2026-10-14. The implementation here is Rust, with a Linux BlueZ
transport and an independent TypeScript editor.

Copyright (c) 2026 iscarelli. MIT license included in
[`licenses/niimbot-web-bluetooth.LICENSE`](licenses/niimbot-web-bluetooth.LICENSE).

Bluetooth operations follow the official BlueZ D-Bus API:

- <https://github.com/bluez/bluez/blob/master/doc/org.bluez.Adapter.rst>
- <https://github.com/bluez/bluez/blob/master/doc/org.bluez.Device.rst>
- <https://github.com/bluez/bluez/blob/master/doc/org.bluez.GattCharacteristic.rst>

No vendor SDK or official NIIMBOT application code is included.
