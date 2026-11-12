# Setting up the Mac server

`bbd` runs on a Mac that is signed in to your Apple ID and stays on. A Mac mini or an old MacBook with the lid closed both work. It needs macOS 13 or later; macOS 14 or later is recommended for Calendar.

## 1. Prepare macOS

- **Messages:** sign in with your Apple ID. On your iPhone, go to Settings → Messages → **Text Message Forwarding** and enable this Mac, so SMS reach it too.
- **Send from your phone number, not your email:** on the iPhone, go to Settings → Messages → Send & Receive and set *Start New Conversations From* to your number. On the Mac, go to Messages → Settings → iMessage, tick your number and set the same option. BlackBubbles always replies from the address a conversation already uses, and each chat header shows it (email is flagged in yellow). Start a new conversation to move a chat from email to your number.
- **Photos:** enable iCloud Photos, or keep a local library.
- **Sleep:** stop the Mac sleeping (System Settings → Energy → *Prevent automatic sleeping when the display is off*).

## 2. Install

```sh
# Apple's compiler and linker (a dialog appears; click Install), then Rust
xcode-select --install
curl https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"

git clone https://github.com/craftyderpysmall/blackbubbles   # this repo's crates/server is the same bbd
cd blackbubbles
packaging/macos/install-server.sh
```

This does the following:
- Builds `bbd`.
- Copies it to `~/Applications/BlackBubbles/bbd`.
- Ad-hoc signs it, so macOS remembers the permissions you grant.
- Installs a LaunchAgent that starts it at login and restarts it if it stops.

## 3. Grant permissions

1. **Full Disk Access.** This is required for Messages, Notes and Voice Memos. Go to System Settings → Privacy & Security → Full Disk Access → **+**, then choose `~/Applications/BlackBubbles/bbd`. (In the file picker, press ⌘⇧G and paste the path.) Then restart it:
   ```sh
   launchctl kickstart -k gui/$(id -u)/com.craftyderpysmall.bbd
   ```
2. **Calendars, Reminders and Photos.** macOS asks the first time a client opens those tabs. Click *Allow*, or allow them later under Privacy & Security.
3. **Automation.** The first time you send a message or edit a note, macOS asks whether `bbd` may control Messages or Notes. Click *Allow*.

## 4. Pair your computers

```sh
~/Applications/BlackBubbles/bbd pair --host <how clients reach this Mac>
```

`--host` is the Mac's address as clients will dial it. It can be a Tailscale name (`my-mac.tail1234.ts.net`), a Tailscale IP, or `my-mac.local` on the same network. The command prints a `blackbubbles://…` string. Paste it into BlackBubbles → Settings → Pair, or run `blackbubbles pair '<string>'`.

Each string works once and expires after 10 minutes. To see or remove paired devices:

```sh
~/Applications/BlackBubbles/bbd devices
~/Applications/BlackBubbles/bbd revoke "<device name>"
```

## 5. Find My

```sh
~/Applications/BlackBubbles/bbd findmy-login you@icloud.com
```

This asks for your Apple ID password and then for 2FA codes. **If no code pops up on your devices**, type `sms` at the prompt and press Enter: Apple texts the code to your phone number instead. On some iOS versions you can also make a code yourself under Settings → your name → Sign-In & Security, if a **Get Verification Code** option is there. It then asks whether to **remember the password** on the Mac. iCloud's web session expires every so often (Find My then said "session expired"); with the password remembered, `bbd` signs itself back in without a new code, because Apple still trusts this sign-in. The password and session tokens are kept in files with mode 600 under bbd's data directory (`icloud/`). Answer `n` to keep only session tokens; you'll then rerun `findmy-login` whenever the session expires. If Apple rejects the saved password (you changed it), `bbd` deletes it at once and never retries it.

## Troubleshooting

- **Logs:** `tail -f ~/Library/Logs/bbd.log`
- **"cannot open … chat.db":** Full Disk Access is missing, or was granted before the binary changed. Remove `bbd` from the list, add it again, then kickstart.
- **Messages not sending:** check System Settings → Privacy & Security → Automation → bbd → Messages.
- **Voice Memos is empty or says the library doesn't exist:** open the Voice Memos app on the Mac once (with Voice Memos turned on under your Apple ID → iCloud), so it downloads your recordings. After that, BlackBubbles can read them even while the app is closed.
- **Contacts show as numbers:** names come from the Mac's Contacts app. Turn on Contacts under your Apple ID → iCloud on the Mac, and check that Contacts.app shows them.
- **Find My "session expired" or "wants a new 2FA code":** run `findmy-login` again (no restart needed). Say yes to remembering the password so it renews itself.
- **Something's wrong and you don't know why:** from the paired Windows PC, in a terminal, run `blackbubbles request '"Diagnose"'`. It prints what `bbd` sees: Notes folders and lock settings, Contacts counts, Voice Memos files and its permissions. It shows counts and settings only, never note text or contact details.
- **`findmy-login` waits for a code that never arrives:** type `sms` at the prompt to get it by text (see step 5).
- **Windows can't find `….local`:** see [WINDOWS_SETUP.md → Let Windows find the Mac](WINDOWS_SETUP.md#3-let-windows-find-the-mac).
