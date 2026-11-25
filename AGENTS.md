# BlackBubbles for Windows

BlackBubbles is a Tauri v2 Windows client and a Rust server, `bbd`, running in the signed-in Mac user's session. This repo is maintained separately from upstream BlackBubbles. Use the Mac server built from this checkout for its server-side fixes.

## Repository map

| Path | Responsibility |
| --- | --- |
| `ui/src` | React pages, shared Radix controls, Tailwind theme, Tauri IPC and the React Query cache |
| `crates/client` | Windows app, tray and CLI, pairing and reconnect, media proxy, desktop notifications and WASAPI audio |
| `crates/proto`, `crates/net` | Protocol 7 types and framing, postcard over WebSocket, pairing and pinned TLS |
| `crates/server` | Pairing, request dispatch, connection-scoped Notes access and the cross-platform mock backend |
| `crates/mac` | Read-only Apple stores, Notes crypto, AppleScript, EventKit and PhotoKit |
| `crates/findmy` | iCloud web authentication and Apple device locations |
| `crates/autofill` | Verification detection, iMessage, IMAP and Shortcut sources, Windows delivery |
| `packaging/macos`, `install.ps1` | Mac LaunchAgent and Windows NSIS installer |

## Build and verify

From the repository root in PowerShell:

```powershell
Push-Location ui
npm ci
npm run build
Pop-Location
cargo build --workspace --locked --features bb-client/custom-protocol
cargo test --workspace --locked --features bb-client/custom-protocol
```

Without `custom-protocol`, the client loads Vite from `http://localhost:5173`. Run `npm run dev` from `ui` in that mode. CI embeds `ui/dist` and builds the per-user NSIS bundle. Run `npm run tauri -- build --bundles nsis` from `ui` to reproduce the bundle locally.

For a Windows smoke test, use separate scratch directories for the mock server and client. Run the mock on loopback and use its one-time URI to pair in Settings. Use `daemon` for tray-only startup and `BLACKBUBBLES_TAB` or `BLACKBUBBLES_FORCE_WIDTH` to open a specific service or check a narrow layout.

```powershell
$mockData = Join-Path $env:TEMP 'bbd-smoke'
cargo run -p bb-server -- --data-dir $mockData pair --host 127.0.0.1 --port 17878
cargo run -p bb-server -- --data-dir $mockData run --mock --bind 127.0.0.1:17878
```

For the client in a separate terminal:

```powershell
$env:BLACKBUBBLES_CONFIG_DIR = Join-Path $env:TEMP 'blackbubbles-smoke'
cargo run -p bb-client --features custom-protocol
```

The mock serves all service types and emits change events after writes. The password for its protected **Diary** note is `1234`. Test protected editing by saving a change, locking it, reopening it and checking that the edit persisted. Check that deleting a note removes it from All and individual folders after a Notes change event.

The Windows app is single-instance, including across installed and dev builds. Coordinate before quitting the user's copy. Use scratch pairing data and leave autofill disabled during UI tests to avoid affecting the clipboard and desktop notifications. Close test processes by their own PID or through their tray control.

## Compatibility and data paths

- Keep the wire protocol at version 7. Keep the request, response and event definitions in `ui/src/lib/bb.ts` aligned with `crates/proto`. Requests use serde's external tags, including bare strings for unit variants.
- Tauri emits `server-event` and `status`. Update or invalidate caches in `ui/src/lib/events.ts`. Reconnect refreshes cached data. Keep network and disk work off UI and async runtime threads where applicable.
- The Mac label comes from `ComputerName` on each handshake, unless `bbd run --name` overrides it. The stored pairing address is separate. Default pairing addresses use `LocalHostName.local`.
- On Windows the config and cache are under `%LOCALAPPDATA%\craftyderpysmall\BlackBubbles`, separate from the install folder. Preserve the owner-and-SYSTEM ACL for the config. `BLACKBUBBLES_CONFIG_DIR` overrides its location.
- The `bb://` media and map proxy appears as `http://bb.localhost` in WebView2. Generate URLs with `convertFileSrc`. Keep the app CSP, blob cache, and media requests aligned with this scheme. The frontend uses HTML drag and drop for attachments.
- Closing the window hides it to the tray. Start at login uses the same executable with `daemon`. Shut down autofill cleanly to restore the clipboard. Keep codes out of history and the cloud clipboard, and keep the detection fixtures faithful to linuxautofill.
- Send messages through AppleScript automation of Messages.app. The Mac chooses the conversation's sending address. Keep the existing phone-number setup instructions. Find My uses Apple's iCloud device list without a Windows beacon.

## Apple Notes and macOS checks

- Read Apple databases with `util::open_readonly`. Treat its immutable WAL fallback as a snapshot and reopen it on each read. Route mutations through the owning Apple app.
- Build note summaries from live note and folder rows. Recently Deleted, missing folders and cloud placeholders stay out of the list. An AppleScript missing-object response is `NotFound`. The client hides the deleted ID while older server views still return it.
- `UnlockNote` verifies a custom Notes password and decrypts the body. `bbd` authorizes protected updates for that note on that connection only. Keep passwords and protected plaintext out of logs, process arguments, persistent client storage and the query cache. Passcode protection relies on Apple's device keychain.
- The plain-text editor saves through Notes.app. For protected notes the native app must have the note unlocked at save time, and protection stays enabled. The write path guards against replacing attachments or tables. Check this flow on a disposable password-protected note on macOS before claiming native compatibility.
- `notecrypt.rs` contains both legacy and keyed-archive decryption paths and published test vectors. `examples/note_probe.rs` reads an encrypted diagnostic dump locally. Use fixtures by default and keep real note dumps and secrets out of the repo.
- Calendar and Reminders use long-lived EventKit stores. Keep the Mac main run loop available and run the server on its worker thread. Photos use PhotoKit and need the corresponding TCC grant.
- See [Mac setup](docs/MAC_SETUP.md) for installation, Full Disk Access, Automation, diagnostics and launchd logs. See [Windows setup](docs/WINDOWS_SETUP.md) for the desktop installer. Report separately what passed in Windows, against the mock, and on a real Mac.

## UI conventions

- Follow [DESIGN.md](DESIGN.md) for tokens, component states, spacing, and workspace layouts. Theme tokens live in `ui/src/index.css`. Shared primitives live in `ui/src/components/ui.tsx`.
- Follow the Claude system in DESIGN.md in its dark mode only: neutral near-black surfaces (`#0c0c0e` canvas), `#2a2a2b` hairlines, white and grey text, and coral as the single brand accent. Use the dark roles listed in DESIGN.md, with no glows, gradients or warm-brown surfaces. Use one coral primary action per workspace. Coral also marks focus, links and the active row.
- Use bundled Cormorant Garamond Medium for serif display in place of Copernicus, Inter for controls and reading in place of StyreneB, and JetBrains Mono for fixed-width detail. Keep Segoe UI Emoji before the bundled subset fallback.
- Check full, compact, and narrow layouts, including keyboard return from a detail pane. Keep readable text selectable, preserve visible focus, and respect reduced motion.
- Keep dependency versions pinned. Use the existing React, Tailwind, Radix, Lucide and Tauri stack. Preserve unrelated changes, and run the relevant build and tests before committing.
