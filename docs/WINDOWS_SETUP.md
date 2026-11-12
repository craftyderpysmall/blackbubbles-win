# Setting up BlackBubbles on Windows

You need Windows 10 or 11, and a Mac running `bbd` (see [MAC_SETUP.md](MAC_SETUP.md)).

## 1. Build and install

BlackBubbles is built from source. Install the tools once:

```sh
winget install Rustlang.Rustup OpenJS.NodeJS.LTS
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then open a **new** terminal, so the new tools are on PATH, and run this from the repository folder:

```sh
powershell -ExecutionPolicy Bypass -File install.ps1
```

The script:
- builds the app and a per-user installer
- installs BlackBubbles to `%LOCALAPPDATA%\BlackBubbles`, with a Start Menu shortcut (no admin needed)
- starts it

Run the script again to update; it closes the running copy first. To uninstall, go to Settings → Apps → Installed apps → BlackBubbles. Your settings are kept in `%LOCALAPPDATA%\craftyderpysmall\BlackBubbles`.

WebView2, which draws the window, comes with Windows 11. On Windows 10 the installer downloads it if it's missing.

## 2. Pair with your Mac

On the Mac, run `~/Applications/BlackBubbles/bbd pair` and copy the `blackbubbles://…` string it prints. The code in it works once, within 10 minutes.

In BlackBubbles, open **Settings → Connection**, paste the string and click **Pair**. The first pairing also turns on **Start at login**.

You can also pair from a terminal:

```sh
blackbubbles pair 'blackbubbles://…'
```

## 3. Let Windows find the Mac

Windows resolves `.local` names itself (mDNS), but only on networks it treats as **Private**. Check:

```powershell
Resolve-DnsName users-MacBook-Pro.local
```

If that fails:
- **Make the network Private:** Settings → Network & internet → Wi-Fi (or Ethernet) → your network → Network profile type → **Private network**.
- **Or skip the name:** pair with the Mac's IP address. Give the Mac a fixed address in your router (a DHCP reservation), then run `bbd pair --host <ip>` on the Mac.
- **Away from home:** use [Tailscale](https://tailscale.com) on both machines and pair with the Mac's Tailscale name.

## 4. Autofill

Verification codes from iMessage/SMS on the Mac work as soon as you're paired. A code arrives as a toast:
- It's already on the clipboard. It's kept out of Win+V clipboard history and the cloud clipboard.
- Click **Paste** to type it into the window you were using.
- After the time set in Settings → Autofill, the clipboard goes back to what you had copied, unless you've copied something else since.

Clipboard contents from password managers (KeePass, KeePassXC, 1Password, Bitwarden) are never saved or put back. Windows doesn't let an app type into windows that run as administrator, so for those, paste with Ctrl+V.

**Gmail:** in Settings → Autofill → Gmail, add the address and an [app password](https://myaccount.google.com/apppasswords).

**iPhone Shortcut endpoint** (for codes while the Mac is off):
1. In Settings → Autofill, click **Enable with a new token**.
2. The first time, Windows Firewall asks whether BlackBubbles may accept connections. Allow **Private networks** only.
3. In the iPhone Shortcut, post to `http://<this PC's name>.local:8787/message` with the `Authorization: Bearer …` header that Settings shows. If the iPhone can't reach the `.local` name, use the PC's IP address and reserve it in your router.

The PC's name is in Settings → System → About (*Device name*).

## 5. Running in the background

- **Close the window:** BlackBubbles keeps running in the tray (the notification area by the clock), so codes and notifications still arrive.
- **Open it again:** click the tray icon, or start BlackBubbles from the Start Menu.
- **Quit for real:** right-click the tray icon → **Quit BlackBubbles**.
- **Start at login:** Settings → Startup. BlackBubbles starts in the tray, without opening the window.

If the tray icon is hidden under the `^` overflow, drag it onto the taskbar to keep it visible.

## 6. Moving over from the Linux PC

The config format is the same. Copy `~/.config/blackbubbles/config.toml` from the Linux PC to `%LOCALAPPDATA%\craftyderpysmall\BlackBubbles\config\config.toml`. BlackBubbles makes the file private (only you and SYSTEM can read it) the next time it starts.

The `[server]` table pairs this PC as the Linux device, so pairing fresh is cleaner: `bbd devices` then lists both PCs separately, and `bbd revoke` can remove either.

## Troubleshooting

- **No toasts:**
  - Check Settings → System → Notifications → BlackBubbles.
  - Focus assist / Do not disturb hides toasts. They still collect in the notification centre.
- **`blackbubbles request …` prints nothing:** run it from Windows Terminal, PowerShell, cmd or VS Code's terminal. A plain mintty Git Bash window has no Windows console to print to.
- **Logs:** start `blackbubbles` from a terminal to see its log there. Set `RUST_LOG=debug` for more.
