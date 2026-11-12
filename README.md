# BlackBubbles for Windows

Your Apple world on **Windows**: Messages, Photos, Notes, Calendar, Reminders, Voice Memos and Find My, plus iOS-style verification code autofill. It is a [Tauri](https://tauri.app) app (Rust core, React UI on WebView2) that talks to a small Rust daemon, `bbd`, on your Mac.

This is the Windows port of [BlackBubbles](https://github.com/craftyderpysmall/blackbubbles) (forked from upstream commit `9918918`). It speaks the same protocol (version 7) to the same `bbd`, so a Mac set up for the Linux client works unchanged. It is not based on BlueBubbles.

> **Status:** early. The upstream client is in daily use on one Mac; this Windows port is new. Messages (with contact names), Notes and Find My run against real data; Photos, Calendar and Reminders are tested against the mock server but not yet on a real Mac. Expect rough edges.

## Features

| | What you get | How it works on the Mac |
|---|---|---|
| **Messages** | iMessage and SMS chats with contact names and group events, history, sending text and files (drag & drop), images inline, Save As | Reads `chat.db` directly and sends through AppleScript |
| **Photos** | Your library and albums in a fast thumbnail grid, full-size viewer, save to disk | PhotoKit, rendered to JPEG (no HEIC on the client) |
| **Notes** | Folders, search, read, edit, new and delete. Password-locked notes open read-only with your Notes password (passcode-locked ones can't) | Reads `NoteStore.sqlite` and decrypts locked notes locally; edits go through AppleScript |
| **Calendar** | Month grid, agenda, create and delete events; reminders with a date show on their day | EventKit |
| **Reminders** | Today / Scheduled / All / Completed, your lists, add, tick off, delete; timed reminders alert on your iPhone too | EventKit (the Reminders app's store) |
| **Voice Memos** | List, play, seek, save | Recordings database and audio files |
| **Find My** | Your Apple devices on a map, optionally with your family's | iCloud Find My |
| **Autofill** | Verification codes and links go straight to your clipboard | Built-in port of [linuxautofill](https://github.com/craftyderpysmall/linuxautofill), see below |

### Services

Settings → Services chooses which of Messages, Photos, Notes, Calendar, Reminders, Voice Memos and Find My BlackBubbles shows (all on by default). A hidden service loses its tab and its notifications; autofill is unaffected.

### Notifications

Settings → Notifications decides exactly what reaches your desktop:
- **Messages:** iMessage and SMS separately, with or without the sender and a preview. You can skip code-only texts (autofill already shows those), stay quiet while the window is focused, and mute single conversations from their header.
- **Other alerts:** calendar reminders N minutes before an event, Reminders when they're due, low battery on any Find My device, new photos, notes or voice memos, and the Mac connection dropping or coming back.
- **Quiet hours:** for example 22:00–07:00.

Notifications are Windows toasts. They keep coming while the window is closed, because BlackBubbles stays in the tray.

### Autofill

This is a faithful Rust port of linuxautofill's detection, and its whole test suite passes unchanged. It also keeps linuxautofill's delivery rules:
- A repeated code is ignored for 120 seconds.
- Your previous clipboard is saved, unless a password manager put it there, and restored afterwards.
- Nothing is typed or opened unless you click the notification.

Codes come from three places:
1. **Every iMessage/SMS the Mac receives.** No iPhone Shortcut is needed if Text Message Forwarding is on.
2. **Gmail over IMAP.** It checks every 0.5 seconds, doesn't mark mail as read, and also finds verification links.
3. **The iPhone Shortcut endpoint** (`POST /message`, same contract as linuxautofill). Use it when the Mac is offline.

On Windows:

| | |
|---|---|
| **Clipboard** | Win32. Codes are kept out of Win+V history and the cloud clipboard, and password-manager content is never saved or restored |
| **Click to paste** | The toast's **Paste** button types the code (SendInput) into the window you were using. Windows doesn't allow typing into windows that run as administrator |
| **Notification** | A WinRT toast with Paste or Open buttons |

**Keep autofill running when the window is closed.** Closing the window hides BlackBubbles to the tray, and Settings → Startup → **Start at login** (on after the first pairing) starts it in the tray when you sign in.

## Design

The UI follows the "Neutral" desktop theme:
- **Look:** monochrome dark and shadcn-style. Colour is used only for state (status dots, low battery, email-address warnings).
- **Shape:** 16 px cards, 10 px controls and pill buttons, with one inverted white primary action per surface.
- **Type and icons:** Inter and Lucide icons are bundled (OFL and ISC licences). Emoji render in colour with Windows' Segoe UI Emoji, including skin tones and ZWJ sequences. A bundled Noto subset fills in emoji newer than Windows' font. Flags show as letter pairs, as they do everywhere on Windows.
- **Motion:** quick decelerating enter and accelerating exit animations.
- **Responsive:** the rail collapses to icons, and Messages, Notes and Find My go single-pane on narrow windows. Calendar stacks its agenda under the month and shrinks events to dots.

## Layout

```
crates/proto     wire protocol (postcard over WebSocket)
crates/net       pinned self-signed TLS, pairing strings
crates/server    bbd, the daemon on your Mac (--mock runs anywhere)
crates/mac       macOS data access: chat.db, NoteStore, Voice Memos, EventKit, PhotoKit, AOSKit
crates/findmy    iCloud Find My sign-in (Apple SRP) and device locations
crates/autofill  linuxautofill port: detection, IMAP, HTTP, Windows delivery
crates/client    the Tauri app's Rust side (blackbubbles: window, tray, pair)
ui/              the React + Tailwind UI (Vite)
packaging/       the Mac server's launchd agent and install script
install.ps1      build and install on Windows (per-user, no admin)
```

## Getting started

1. **Mac:** follow [docs/MAC_SETUP.md](docs/MAC_SETUP.md). Skip this if your Mac already runs `bbd` for the Linux client.
2. **Windows:** follow [docs/WINDOWS_SETUP.md](docs/WINDOWS_SETUP.md). In short: install Rust, Node and the MSVC Build Tools, run `install.ps1`, then pair in Settings.

You can try the client without a Mac by running the mock server:

```sh
cargo run -p bb-server -- pair --host 127.0.0.1      # prints a pairing string
cargo run -p bb-server -- run --mock --mock-codes 20 # fake data + a fake code text every 20 s
(cd ui && npm ci && npm run build)
cargo run -p bb-client --features custom-protocol    # Settings → paste the string → Pair
```

## Security

- **Connection:** the client pins the server's self-signed certificate (SHA-256) from the pairing string, so no CA is trusted.
- **Pairing codes:** they expire after 10 minutes, work once, and are compared in constant time.
- **Device tokens:** the server stores only their hashes. `bbd devices` lists paired devices and `bbd revoke` removes one.
- **Secrets:** the client config holds secrets. It lives in your local (not roaming) profile, and its access list allows only you and SYSTEM; BlackBubbles enforces that every time it reads or writes the file.
- **Remote access:** use [Tailscale](https://tailscale.com) or another VPN rather than port forwarding.

## Caveats

- **Find My** uses undocumented Apple endpoints, implemented clean-room from public research (OpenHaystack, macless-haystack). It can break whenever Apple changes them.
- **Messages** are sent through AppleScript, so tapbacks, edits and typing indicators aren't supported.

## License

MIT
