# BlackBubbles for Windows: notes for Claude and the owner

This repo is the **Windows port** of [BlackBubbles](https://github.com/craftyderpysmall/blackbubbles), forked from upstream commit `9918918` on 2026-11-11 with fresh history. BlackBubbles bridges the owner's Apple world to a desktop:
- **On the Mac:** its own Rust server, `bbd`.
- **Protocol:** its own (postcard over pinned-TLS WebSocket), not BlueBubbles.
- **Client:** a Tauri v2 + React app, here for Windows only.

## How the client fits together

- **Rust side** (`crates/client`, binary `blackbubbles.exe`, GUI subsystem):
  - No subcommand opens the window. `daemon` starts in the tray (what Start at login runs). `pair <uri>` pairs. The hidden `request '<proto json>'` sends one request to the Mac and prints the reply.
  - CLI output reaches the terminal through `AttachConsole(ATTACH_PARENT_PROCESS)`. That works in Windows Terminal, PowerShell, cmd and VS Code, but not in a mintty Git Bash window.
  - **Use `request` to look at the owner's real data instead of guessing**, e.g. `blackbubbles request '"Diagnose"'` or `'"ListContacts"'`.
  - `commands.rs` holds `AppState` and every Tauri command.
- **Requests:** the frontend calls the `request` command with proto JSON (serde's externally tagged enums, e.g. `{"ListChats":{"limit":300,"before":null}}`; unit variants are bare strings like `"ListCalendars"`). Types are mirrored by hand in `ui/src/lib/bb.ts`; keep them in sync with `crates/proto` (which is frozen anyway).
- **Events:** server `Event`s are emitted as `server-event` and connection changes as `status`. `ui/src/lib/events.ts` applies them straight to the React Query cache, so nothing polls.
- **Media:** the `bb://` scheme (`blob.rs`) is `http://bb.localhost/…` on Windows, which the CSP allows.
  - It serves `blob/<Kind>/<id>` from chunked `GetBlob`, with a 160 MB LRU and a 24-request cap.
  - It proxies OpenFreeMap (`map/<path>`) with a disk cache. MapLibre's `transformRequest` rewrites `tiles.openfreemap.org` URLs to it.
  - The frontend builds URLs with `convertFileSrc(path, "bb")`.
- **Uploads:** attachments go to `send_attachment` as a raw-body invoke (`x-chat`/`x-name` headers). The window sets `dragDropEnabled: false` so HTML5 drag-and-drop gives File objects.
- **Audio:** voice memos play through rodio (WASAPI) on its own thread (`audio.rs`), polled by the UI 4×/s while loaded.
- **Emoji:** the stack is Inter, then **Segoe UI Emoji** (complete; shapes FE0F and ZWJ), then the bundled `@fontsource/noto-color-emoji` subsets renamed "BlackBubbles Emoji" (newer emoji only).
  - Split subsets can't shape emoji + U+FE0F, so they must never come first.
  - Flags render as letter pairs (a Windows limitation). A full single-file Noto COLRv1 would fix it, but none was available at a sane size (2026-11-11). Ask before adding one.
  - The mock's reminder "Flight to Lisbon ✈️" is the regression check.
- **Liveness (from upstream, keep):**
  - `bb_net::connect` tries every resolved address, staggered by 250 ms.
  - The handshake has one 15 s deadline.
  - Pings go every 15 s; 45 s of silence counts as dead.
  - Sends time out after 10 s, and backoff is capped at 10 s.
  - `AppState::connect` closes the old client before wiring the new one.

## Windows platform (the parts that differ from upstream)

- **Data:** `%LOCALAPPDATA%\craftyderpysmall\BlackBubbles\{config,cache}` (`ProjectDirs("", "craftyderpysmall", "BlackBubbles")`, local not roaming).
  - It's deliberately **not** `%LOCALAPPDATA%\BlackBubbles`: that's the NSIS per-user install folder, and an uninstall could take the pairing with it.
  - `BLACKBUBBLES_CONFIG_DIR` overrides the config folder.
- **Tray** (`main.rs` `build_tray`):
  - Left-click shows the window. The menu has Open, a status line (`TrayStatus`, updated from `on_status`; also the tooltip) and Quit.
  - `CloseRequested` hides the window instead of closing it.
  - The window starts `visible: false` and setup shows it, except under `daemon`.
  - The event loop never returns, so cleanup is on `RunEvent::Exit`: `Autofill::shutdown()` restores the clipboard.
- **Start at login:** `tauri-plugin-autostart` (HKCU Run, args `daemon`), used Rust-side through `get_autostart`/`set_autostart`. The first successful pair turns it on.
- **Toasts** (`tauri-winrt-notification`):
  - AUMID `com.craftyderpysmall.blackbubbles` (the bundle identifier). `platform::register_app` writes `HKCU\Software\Classes\AppUserModelId\<id>` (DisplayName, IconUri → a PNG under the data folder) at every start, so toasts work unpackaged.
  - The crate builds the XML through the DOM (`SetInnerText`), so untrusted text needs no escaping.
  - `notify()` blocks until a click or `timeout_secs`, keeping the `Deliverer` contract.
  - Clicking a plain toast shows the window (`on_notification_click`).
- **Clipboard:**
  - Codes are copied with arboard's `exclude_from_history().exclude_from_cloud()`, so they stay out of Win+V and the cloud clipboard.
  - `snapshot` refuses password-manager content: any of `ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore`, or `CanIncludeInClipboardHistory` = 0. Otherwise it saves files (CF_HDROP), then text, then a DIB.
- **Typing:**
  - While a code's toast is up, `notify` polls the foreground every 100 ms and remembers the last **typing target**: any visible window except the shell, toast and Start/search hosts and komorebi (`is_typing_target`, `NOT_TARGETS`). BlackBubbles' own window counts, e.g. its message box.
  - A toast click leaves `explorer.exe [ApplicationManager_DesktopShellWindow]` in front (verified 2026-11-12), so `type_text` brings the target back with `bring_to_front`: plain `SetForegroundWindow` (worked directly in every test), then `AttachThreadInput`, then an Alt tap.
  - It types only once the target is really in front. With no target, or if switching back fails, it types nothing and the code stays on the clipboard.
  - `RUST_LOG=bb_autofill=debug` logs every foreground change and verdict while a toast is up; `paste:` lines are at info.
  - UIPI blocks typing into elevated windows.
- **Firewall:** the Shortcut endpoint binds `0.0.0.0:8787`, and Windows Firewall prompts on first listen. Tell the owner to allow Private networks only.
- **mDNS:** Windows resolves `.local` natively, on Private networks only.

## Status (2026-11-11)

**Builds on Windows, and all 39 tests pass** (2026-11-12; `--exclude bb-mac`). The tests include the verbatim detect suite, the `write_private` ACL test checked with real `icacls`, and PNG encoding. The APIs used were checked against the crate sources: clipboard-win 5.4.3, arboard 3.7, windows 0.62.2, tauri 2.12.5, autostart 2.7, single-instance 2.5.5, tauri-winrt-notification 0.8.1.
- **Verification status:**
  1. **Tests:** done.
  - **Mock, partly done (2026-11-11):**
    - The CLI `pair` went over pinned TLS, the config came out with an owner + SYSTEM only access list, and `request` output reached the terminal.
    - The window rendered Messages (Segoe colour emoji incl. 🏕️), Reminders, Calendar, Photos (`bb://` blobs), Find My (map via `bb://map`) and Settings. These were captured with `PrintWindow` on the app's own window only, relaunching per tab with `BLACKBUBBLES_TAB`.
    - The AUMID registry key was written.
    - Not yet checked on the mock: send, paste and drag-drop of files into a chat, Save As, voice playback.
  2. **Code toast, verified by the owner on the mock (2026-11-12):**
     - Toasts show as BlackBubbles.
     - Paste types into the window the owner was in (Chrome, BlackBubbles' message box).
     - The clipboard is restored after `clear_after`, and codes stay out of Win+V.
     - Fixed along the way: `clear_after`, `notify` and `from_server` used to need a restart, and they now apply when saved (`Autofill::set_options`, `Client::set_autofill(Option)`). Paste used to type into the shell after the toast click.
  3. **Tray and Start at login, verified by the owner (2026-11-12):** close hides, the tooltip shows the status, left-click reopens, a second launch focuses, and the switch adds and removes the HKCU Run value.
  5. **Real Mac:** `Resolve-DnsName users-MacBook-Pro.local`, pair with a fresh `bbd pair`, `blackbubbles request '"Diagnose"'` from Windows Terminal, a real iMessage code, the Shortcut endpoint from the iPhone, and Gmail.
  6. **Installer:** `install.ps1`, reboot, and BlackBubbles is in the tray, with toasts named BlackBubbles with its icon.

### The Mac side (from upstream; `bbd` unchanged)

The owner's Mac runs `bbd` at `users-MacBook-Pro.local:7878` (macOS). Logs are in `~/Library/Logs/bbd.log` on the Mac. **Debug against real data, not guesses:** `blackbubbles request '"Diagnose"'` reports Notes, Contacts and Voice Memos structure and bbd's TCC grants. Repeated guessing at Apple formats cost the owner many rounds upstream.

Confirmed working upstream (Linux client) on real data:
- Messages, with contact names (the number-as-`display_name` bug is fixed)
- Find My sign-in and devices, and the family toggle. The 2FA pop-up never arrives for this account, and their iOS has no "Get Verification Code"; `findmy-login` → type `sms`.
  - The iCloud web session expires every so often ("session expired (421 Misdirected Request)"). Since 2026-11-07 `Session::reauthenticate` tries `validate` (cookies, saved after every refresh), then `accountLogin`, then, if `findmy-login` was told to remember the password (`icloud/credentials.json`, mode 600), an SRP sign-in with the trust token, which needs no 2FA while Apple trusts it. Password sign-ins are rate-limited to one per 10 min, and a rejected password is deleted at once (never retried: account lockout). bbd drops its session after an error, so a new `findmy-login` takes effect without a restart. Unverified on real data until the next expiry.
- Notes list, edit and Delete (Delete needs bbd's Automation grant for Notes, which it has)

Fixed from real data, waiting for the owner's next update to confirm:
- **Phantom "E" folder:** `ZNEEDSINITIALFETCHFROMCLOUD = 1` (a never-downloaded iCloud placeholder). Folders and notes with that flag are now hidden.
- **Undeletable "New Note" dated Dec 31 2000:** a note with `ZFOLDER` NULL and modification date 0. Notes without a folder are now hidden, as Notes.app does. Don't try to delete it (AppleScript can't see it, and we never write Apple's databases).
- **Locked notes said "wrong password":** the account uses the **account-level** scheme. Account row 72 has `ZCRYPTOSALT` 16 B, `ZCRYPTOITERATIONCOUNT` 20000 and `ZCRYPTOVERIFIER` 24 B. The note has iterations 0, a 16-byte `ZCRYPTOWRAPPEDKEY` and its own salt; IV and tag are on `ZICNOTEDATA`. We used the note's iterations (0), so every password failed. Now `notecrypt::decrypt` unwraps the account verifier first (certain right/wrong-password answer), then tries a few note-key constructions, accepted only if the AES-GCM tag verifies. The owner then reported that message on every attempt (2026-11-07). **Root cause, from `NoteCryptoDump`:** the account uses **cipher version 2** (`ICAccountData.ZLOCKEDNOTESMODE` 1, `ZSUPPORTSV1NEO` 1, note `ZMINIMUMSUPPORTEDNOTESVERSION` 13). A v2 note's `ZICNOTEDATA.ZDATA` is not ciphertext but an NSKeyedArchiver'd `ICCryptoEncryptionObject`: `unauthenticatedMetadata` plist {passphraseSalt 32 B, passphraseIterationCount 20000, hint}, `metadata` plist {cipherVersion 2, objectIdentifier = note ZIDENTIFIER}, `wrappedEncryptionKey` 40 B (RFC 3394 of a 256-bit key) and `encryptedData`. `ICAccountData.ZCRYPTOPASSPHRASEVERIFIER` is a protobuf (field 1 = Apple date, field 2 → 5 = plist {key 40 B, authenticatedData, metadata with the same salt}). The note row's 16-byte salt/wrapped key and the account row's 16-byte salt/24-byte verifier are legacy (v1) leftovers; the note row's IV/tag equal the data row's. `notecrypt::parse_v2`/`decrypt_v2`: PBKDF2-SHA256(password, passphraseSalt, iterations) → 32 (or 16) B KEK → AES-KW unwrap (certain wrong-password check) → AES-GCM over `encryptedData`, trying a few IV/tag/AAD layouts, each authenticated. The owner's password unwrapped the note key on real data (note_probe, 2026-11-07), but every layout we guessed failed. The real one, from 6over3/NotesKit `docs/encryption-research.md` (Apple calls it "Cipher v1 Neo"): `encryptedData` = ciphertext | **32-byte** IV | 16-byte tag, AES-256-GCM, AAD = raw `metadata`. Its published KEK/DEK vector is a unit test. Notes whose `metadata` has an `accountKeyIdentifier` are device-passcode ("V2"): the key is in the Data Protection Keychain (Notes' entitlement only), so bbd says so instead of asking for a password. bbd logs the layout that worked (`notes: unlock … decrypted via v2 …`). To dig further: `blackbubbles request '{"NoteCryptoDump":{"id":"<note id>"}}' > dump.json` (every field of the locked note, its note data and account rows, blobs in hex; still encrypted), then have the owner run `cargo run --release -p bb-mac --example note_probe -- dump.json` and type the password. It checks the v2 verifier and note key, then searches body layouts, printing constructions and lengths only (never note text).

Known, not fixable here:
- **Voice Memos:** the Mac has 0 recordings (`Diagnose`); iCloud isn't syncing them between the owner's iPhone and Mac. The owner said to leave it.
- **Smart folders** (`ZFOLDERTYPE` = 2, `ZSMARTFOLDERQUERYJSON`) list with 0 notes; their queries aren't evaluated.
- **Contacts:** the Mac's address book has 51 phone numbers, and every chat handle that's in it resolves. The other numbers aren't contacts.

Not yet exercised on real data: Photos (PhotoKit), Calendar and Reminders (EventKit; TCC grants exist), sending messages and attachments, Save As, the Tauri client on macOS.


## Layout

```
crates/proto     wire protocol (FROZEN at PROTOCOL_VERSION = 7)
crates/net       self-signed TLS + SHA-256 pinning, pairing URIs, framed WebSocket; write_private/restrict_to_owner
crates/server    bbd (vendored): pairing/devices, sessions, --mock backend (runs on Windows)
crates/mac       bbd's macOS data access (vendored, macOS-only)
crates/findmy    iCloud Find My (vendored, used by bbd)
crates/autofill  detect (verbatim port + tests), IMAP, HTTP endpoint, Deliverer, Windows Platform
crates/client    Tauri app: main.rs (CLI, tray, plugins), commands.rs, blob.rs, audio.rs, conn.rs, notify.rs, config.rs
ui/              React UI: src/lib (bb.ts bridge, events.ts, app.tsx, format.ts), src/components, src/pages/*
packaging/macos  bbd's launchd agent + install script
docs/            WINDOWS_SETUP.md, MAC_SETUP.md
install.ps1      build + silent per-user NSIS install
```

## Build, test, run

From cmd (PowerShell: `$env:VAR = ...` instead of `set`):

```sh
cd ui && npm ci && npm run build && cd ..
cargo build --workspace --features bb-client/custom-protocol
cargo test --workspace --exclude bb-mac --features bb-client/custom-protocol   # bb-mac is macOS-only (Unix perms in its tests)
cargo run -p bb-server -- --data-dir %TEMP%\bbd pair --host 127.0.0.1 --port 17878
cargo run -p bb-server -- --data-dir %TEMP%\bbd run --mock --mock-codes 20 --bind 127.0.0.1:17878
set BLACKBUBBLES_CONFIG_DIR=%TEMP%\bbc
cargo run -p bb-client -- pair "blackbubbles://…"
cargo run -p bb-client --features custom-protocol
```

- **custom-protocol:** without it, Tauri loads `http://localhost:5173` (run `npm run dev` in `ui/`). Switching it rebuilds the tauri crates, so pick one mode per session. CI and release builds use it.
- **Installer:** `npm run tauri -- build --bundles nsis` from `ui/` (the script `cd`s to the repo root). `install.ps1` wraps it. Output: `target\release\bundle\nsis\*-setup.exe`.
- **Mock server:** `--mock` serves fake chats, events, notes, memos and photos (generated BMP/WAV) and Find My devices. Writes echo events like the Mac does. `--mock-codes N` sends a fake verification-code text every N seconds. The locked note "Diary" opens with 1234.
- **Developer environment variables:**
  - `BLACKBUBBLES_CONFIG_DIR`
  - `BLACKBUBBLES_TAB=<messages|photos|notes|calendar|reminders|voice|findmy|settings>`
  - `BLACKBUBBLES_OPEN_FIRST_CHAT=1`
  - `BLACKBUBBLES_FORCE_WIDTH=<px>`
- **Single instance:** a test build and an installed copy share the AUMID and single-instance key. Quit the installed one from the tray before running a dev build, or the dev build just focuses it and exits.
- **The owner may be using the PC.** Don't send keys or take screenshots blindly. Check which window is in front first.
- **Clipboard caution:** autofill and paste tests touch the owner's real clipboard.
  - Save it first (text, files and image), and confirm it was restored afterwards.
  - Win+V history holds the owner's items: don't clear it.
  - For UI-only tests, set `[autofill] enabled = false` in the scratch config.
- **Sound:** mock voice memos are tones. Mute before testing playback, and unmute afterwards.
- **Don't** kill `blackbubbles.exe` blindly; it may be the owner's running copy. Use Quit in the tray, or kill your dev build by PID.

## Standing decisions (don't relitigate)

- **Separate repo.** The owner won't touch upstream blackbubbles again. Don't propose upstreaming.
- **The protocol is frozen at 7.** The Mac runs upstream's `bbd`, which won't change. Never edit `crates/proto` or the wire behaviour of `crates/net`; a bump means "protocol version mismatch" and nothing connects. `crates/server`, `crates/mac` and `crates/findmy` are vendored unchanged (the mock is how we test).
- **Stack:** Tauri port (Rust core + the React UI), chosen over a C#/.NET rewrite on 2026-11-11.
- **Background:** one process, tray + Start at login (HKCU Run → `blackbubbles daemon`), not a separate daemon or service.
- **No BlueBubbles code or API.** Our own `bbd` server and protocol.
- **Messages are sent through AppleScript only.**
  - No Private API helper: no injecting into Messages.app, and SIP stays on. The owner decided this.
  - Consequence: no edit, unsend, tapbacks, typing indicators or read receipts. Don't propose a helper again.
- **Sending address:** the owner wants iMessages to go from their **phone number, not email**.
  - BlackBubbles replies from the address each chat already uses, and the Mac's Messages settings decide that address. The setup steps are in `docs/MAC_SETUP.md` and the Settings screen.
  - Each chat reports `my_address`; the UI shows it and flags email with the warning colour.
- **No Find My beacon.** The BLE beacon for the Linux PC (and its GSA/anisette sign-in, key crypto and systemd unit) was removed on 2026-11-06 at the owner's request; it's in upstream's git history (before protocol 6). Don't add one for the Windows PC either. Find My shows Apple devices from iCloud only.
- **Autofill:** a built-in port of [linuxautofill](https://github.com/craftyderpysmall/linuxautofill).
  - Sources: iMessage/SMS from the Mac, Gmail IMAP, and the iPhone Shortcut endpoint.
  - Desktop: Windows only (`crates/autofill/src/platform.rs`; a no-op `Headless` platform elsewhere so tests run anywhere).
  - Detection must stay a faithful port; `crates/autofill/tests/detect.rs` is the original suite, copied verbatim.
- **Performance is a requirement:** never block the UI thread.
- **Apple's SQLite stores** are opened with `util::open_readonly`. If a WAL database's `-shm` is gone (the app isn't running), a plain read-only open fails ("unable to open database file"; the owner hit this with Voice Memos). It then falls back to `immutable=1`, which is a snapshot, so callers must not cache that connection.
- **AppleScript** runs through `util::osascript` with a 30 s timeout. A hang almost always means macOS is showing a permission prompt on the Mac's screen (Delete in Notes hung this way).
- **Secrets:** `bb_net::write_private` gives files a protected DACL (owner + SYSTEM only, SDDL `D:P(A;;FA;;;OW)(A;;FA;;;SY)`) before writing; `Config::load` re-applies it, so a config copied in by hand is fixed rather than refused.

## Design: the "Neutral" theme

These tokens (originally from the owner's Hyprland config, which doesn't exist on this PC) are the source of truth:
- **Palette:**
  - Surfaces: bg `#0a0a0a`, surface `#171717`, raised `#212121`.
  - Borders: border `#262626`, border-strong `#404040`.
  - Text: fg `#fafafa`, muted `#a1a1a1`, subtle `#737373`.
  - Primary: `#fafafa` on `#171717`.
  - Semantic: danger `#e5787a`, success `#8ccf9a`, warning `#e6c879`, info `#82a9e0`, magenta `#b49ae6`, cyan `#7cc6cf`.
- **Colour:** monochrome first. Semantic colours only for state (dots, badges), never large fills.
- **Layering:** layering is lightness, so a darker surface never sits on a lighter one. Every card has a 1px border; focus steps up to border-strong, never a coloured border.
- **Shape:** cards 16px, inner controls 10px, buttons/badges fully rounded. **One inverted white primary per surface**; everything else is ghost or outline.
- **Type:** Inter for UI, JetBrainsMono Nerd Font for mono. Headings semibold fg, body muted.
- **Spacing:** 4px grid, 8 inner gap, 16 outer gap and card padding.
- **Motion:** enter decelerating at ~250–350ms, exit accelerating at about half that, scale changes within 94–100%.
- **Depth:** soft, large, offset-down black shadows, not glow.
- **Icons:** Lucide (`lucide-react`), the set shadcn uses.
- **Implementation:** the tokens are Tailwind `@theme` values in `ui/src/index.css` (`bg-surface`, `rounded-card`, `ease-decel`, `animate-enter`…). The kit is `ui/src/components/ui.tsx` (Button variants primary/ghost/outline/danger, Input, Switch, Badge, Chip, Card, Dialog…). Use them; don't add ad-hoc colours. The decel/accel curves are exact beziers from the original theme; keep them.

## Client features (all ported; keep them working)

Navigation and layout:
- **Rail:** collapses to icons below about 1040px. The active tab is the primary pill; there's a connection status dot.
- **Responsive breakpoints:**
  - Below 760px: Messages, Notes and Find My go single-pane with a back button.
  - Below 980px: Calendar stacks its agenda under the month, and cells show dots only.
  - Photos: the grid is justified to fill the width.

Screens:
- **Messages:**
  - Chat list with avatars, search and time.
  - Thread grouped by day, with monochrome bubbles: yours are inverted, SMS you sent are outlined, received ones sit on raised.
  - "Load earlier", images inline, other files download.
  - Drag-and-drop to send.
  - Paste: WebView2 puts pasted images and Explorer-copied files in `clipboardData.files`. If it doesn't but the clipboard advertises an image or file, the composer falls back to `clipboard_files`/`clipboard_file` (commands.rs → `bb_autofill::platform::clipboard_files`: CF_HDROP files ≤100 MB, else the image as PNG). Pasted files are staged as removable thumbnails above the composer and go out with Send/Enter.
  - Unread (2026-11-07): `Chat.unread` = chat.db `is_read = 0`, not from me, visible messages only. The watcher polls the counts every 2 s and sends `UnreadChanged` when they change (read on the iPhone). Messages' AppleScript can't mark anything read, so opening a chat here stores a local "seen" time (`localStorage` `bb.seenChats`, shared by every window of the app): the dot hides until something newer arrives, and the iPhone still shows it unread. An unread row has a semibold title, fg preview and time, and a white dot; the header shows "N unread".
  - Header badges show the service and `my_address` (email flagged), plus a per-chat mute bell.
  - Group events ("You left the conversation", "Alex added Anthony", renames, group photo) are centred lines in the thread and the chat-list preview. chat.db `item_type` 1/2/3 + `group_action_type` → `Message.group_event` (protocol 4). Other item types (location sharing, FaceTime…) are skipped. The mock has a "Weekend trip" group with each kind.
  - Contact names (fixed 2026-11-06 from real data): the server used to send the chat number as `display_name` for one-on-one chats, which hid every contact name. `display_name` is now only a real group name; the UI's `groupName()` also ignores a name equal to a participant or `chat\d+`. `bbd` reads `~/Library/Application Support/AddressBook/{,Sources/*/}AddressBook-v22.abcddb` (Full Disk Access covers it) and answers `ListContacts` with handle → name for every handle in chat.db. Matching uses lowercase emails and the last 10 digits of phone numbers (`crates/mac/src/contacts.rs`). The UI's `useNames()` applies them to titles, avatars, sender labels and group events; the notifier too.
  - Save As…: `save_blob_as` opens a native dialog (tauri-plugin-dialog, Rust side, so no JS permission). Attachments have a right-click menu (Open, Save As…). The web view's own image menu is suppressed, because its "Save Image As…" can't save bb:// images.
  - Text selection app-wide is white with black text; inside white surfaces (your iMessage bubbles, `.bb-inverted`) it flips to black with white text.
- **Photos:** album pills, lazy thumbnails with an LRU cache, video and favourite badges, lightbox with arrow keys and Save As….
- **Notes:** folder chips, search, plain-text editor (first line is the title), Ctrl/⌘+S, "Edited/Saved" badge, New note. Locked notes show a lock, and opening one asks for the Notes password: `UnlockNote` decrypts it read-only (`crates/mac/src/notecrypt.rs`: PBKDF2-SHA256 → RFC 3394 unwrap → AES-128-GCM with 16-byte IV; per-note and account-level schemes, see Status). The password is never stored. Notes locked with the device passcode can't be opened (keychain-held key). `bbd` still refuses `UpdateNote` on locked notes. Right-click a note → Delete (with a confirm dialog) moves it to Recently Deleted via AppleScript. The note leaves every list at once and stays hidden (a `pending` set) until the request finishes; then lists refetch from the Mac. `bbd` waits up to 10 s after the AppleScript for Notes.app to save its database, because the old immediate refetch saw the note still there and left stale entries in some folder views. The Notes watcher fingerprints COUNT + SUM(`Z_OPT`) + MAX(modified) every 2 s, because a move to Recently Deleted doesn't change the modification date. The mock's locked note ("Diary") opens with the password 1234.
- **Calendar:** month grid with today as a pill, day agenda, delete. Reminders with a due date show on their day as hollow rings (events are dots) and in the agenda with a checkbox. The "New" dialog switches between Event (title, location, notes, all day, times, calendar) and Reminder (title, notes, date, optional time, list, priority).
- **Reminders:** Today (includes overdue) / Scheduled (grouped by day) / All (grouped by list) / Completed tiles with counts, the owner's lists with colours, quick add (title + optional date/time), tick off (stays visible ~1 s), delete, "Show completed". Completed history is only fetched when shown. Timed reminders get an EventKit alarm so they also alert on the iPhone.
- **Voice Memos:** search, play, seek, save; the player card's primary is play/pause.
- **Find My:** Settings → Services → Find My → "Show family members' devices" (`find_my_family`, default on) sends `include_family` = iCloud's `fmly` flag; the server caches each variant separately. Full-bleed MapLibre vector map with OpenFreeMap's `dark` style (no API key; all requests go through the `bb://map/` cache proxy). The device list floats over the map as a translucent panel (hideable). Markers are round device-icon badges; the selected one is inverted with its name and an accuracy ring. A detail card shows last seen, accuracy, battery (coloured only when low), coordinates and "Open in OpenStreetMap". Refreshes every 60 s. CARTO's basemaps were tried and now need an API key; don't go back to inverting light OSM tiles (the owner found it forced).
- **Settings:** sections for Connection/pairing, Sending address help, Services (which Apple services show; all on by default; a hidden service loses its tab and notifications), Notifications (all categories incl. Reminders due, quiet hours, mute list; groups dim when their service is hidden), Autofill (sources, silent mode, Shortcut token, Gmail), About.

Behaviour:
- **Toasts:** bottom-right cards.
- **Times:** 12-hour American ("2:05 PM") everywhere, by the owner's request (`format.ts`).
- **Right-click:** the web view's own menu (Reload, Inspect Element…) is suppressed app-wide except in text fields and on selected text (`main.tsx`). Use Radix context menus for real menus.
- **EventKit and the main run loop:** bbd's main thread runs the Core Foundation run loop (`bb_mac::run_main_loop`); the server runs on its own thread (real backend on macOS only). EventKit does part of its work on the main queue: before, with tokio on the main thread, `reset()`/`refreshSourcesIfNecessary()` and even *releasing* an `EKEventStore` hung every later EventKit call, and a long-lived store never saw iPhone changes (2026-11-07, two broken builds). Each of Calendar, Reminders and their watchers keeps one long-lived store (never released); watchers poll every 10 s. `with_store` gives up after 15 s with "isn't responding" instead of hanging. Unverified on the Mac until the owner rebuilds.
- **Primary process:** the app is single-instance (`tauri-plugin-single-instance`; a second launch shows the window, a second `daemon` does nothing), so the app always does autofill and notifications. `primary.lock` is kept so a CLI `request` never starts autofill.
- **Updating:** `install.ps1` stops the running copy before installing, because Windows can't replace a running exe.

## Working agreements

- **Branch and commits:** commit to `main`. CI (`.github/workflows/ci.yml`, windows-latest: UI build, cargo build and test, NSIS bundle) must be green. End commit messages with the `Co-Authored-By` trailer given in the session.
- **Testing:** test logic with fixtures. Mock anything Apple-side through `bbd --mock`, which runs on Windows.
- **Check APIs:** read the crate source in `%USERPROFILE%\.cargo\registry` (or `ui\node_modules`) before using new versions or APIs.
  - Tauri is 2.x; stay on 2.
  - rodio 0.23 uses `DeviceSinkBuilder` and `Player`.
  - JS dependencies are pinned exactly in `ui/package.json` with a lockfile, and CI runs `npm ci`.
- **Reporting:** be explicit about what was verified on real data, against the mock, or only compiled.
- **Questions:** ask before large or irreversible choices.
