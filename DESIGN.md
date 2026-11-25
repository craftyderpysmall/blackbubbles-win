---
version: alpha
name: BlackBubbles
description: A black desktop workspace with editorial serif headings, generous spacing, and a restrained silver-violet accent.
colors:
  canvas: "#000000"
  navigation: "#08090b"
  surface: "#0e0f13"
  elevated: "#14161c"
  recessed: "#060709"
  hover: "#1a1c25"
  selected: "#212236"
  divider: "#242630"
  border: "#3c4050"
  control-border: "#666b80"
  ink: "#f5f5fa"
  body: "#d1d3df"
  muted: "#a8acbd"
  subtle: "#9498ab"
  primary: "#f1f2fa"
  primary-hover: "#e2e4f8"
  primary-pressed: "#cbd0ee"
  on-primary: "#0b0c11"
  disabled-surface: "#1a1c25"
  disabled-ink: "#7e8294"
  accent: "#b7b9f2"
  accent-strong: "#d3d4ff"
  selection: "#c9cbf8"
  on-selection: "#101118"
  success: "#87c6ae"
  warning: "#e2bf85"
  danger: "#ee9da8"
  danger-surface: "#2c1920"
  scrim: "rgba(0,0,0,0.68)"
typography:
  display-lg:
    fontFamily: "Cormorant Garamond, Georgia, Times New Roman, serif"
    fontSize: 64px
    fontWeight: 500
    lineHeight: 1.05
    letterSpacing: -1.5px
  display-md:
    fontFamily: "Cormorant Garamond, Georgia, Times New Roman, serif"
    fontSize: 48px
    fontWeight: 500
    lineHeight: 1.1
    letterSpacing: -1px
  display-sm:
    fontFamily: "Cormorant Garamond, Georgia, Times New Roman, serif"
    fontSize: 36px
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: -0.5px
  editorial-title:
    fontFamily: "Cormorant Garamond, Georgia, Times New Roman, serif"
    fontSize: 28px
    fontWeight: 500
    lineHeight: 1.2
    letterSpacing: -0.3px
  title:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 18px
    fontWeight: 500
    lineHeight: 1.4
  body:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.55
  control:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 14px
    fontWeight: 500
    lineHeight: 1.4
  caption:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.4
  code:
    fontFamily: "JetBrains Mono, Cascadia Mono, monospace"
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.6
rounded:
  small: 6px
  control: 8px
  panel: 12px
  feature: 16px
  pill: 9999px
spacing:
  xxs: 4px
  xs: 8px
  sm: 12px
  md: 16px
  lg: 24px
  xl: 32px
  xxl: 48px
  section: 64px
  hero: 96px
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: 10px 16px
    height: 40px
  button-secondary:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.ink}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: 10px 16px
    height: 40px
  text-input:
    backgroundColor: "{colors.recessed}"
    textColor: "{colors.ink}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: 10px 14px
    height: 40px
  panel:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.body}"
    typography: "{typography.body}"
    rounded: "{rounded.panel}"
    padding: 24px
  workbench:
    backgroundColor: "{colors.recessed}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.feature}"
    padding: 32px
  status-pill:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.body}"
    typography: "{typography.caption}"
    rounded: "{rounded.pill}"
    padding: 4px 10px
---

# BlackBubbles design

BlackBubbles is a desktop workspace for browsing and working with your Apple services. A true-black canvas, spacious serif headings, and a cool silver-violet accent give it a consistent identity. The note, conversation, photograph, schedule, or recording takes visual priority after the page title.

Give each page a clear editorial heading and let the working area take up most of the window. Use layered graphite surfaces, pale buttons, and clearly marked states. Keep every interface background solid, including empty states, media controls, and dialogs.

## Palette and surface hierarchy

| Role | Token | Application |
| --- | --- | --- |
| Canvas | `canvas` | True black around the workspace and behind its heading. |
| Navigation | `navigation` | Global bar, with a `divider` along the lower edge. |
| Content | `surface` | Lists, grouped settings, reminder tiles, album controls. |
| Raised | `elevated` | Menus, popovers, dialogs, compact toolbars, secondary buttons. |
| Recessed | `recessed` | Writing area, code, map fallback, media, and input backgrounds. |
| Hover | `hover` | A momentary lift on an interactive row or control. |
| Selected | `selected` | A quiet violet fill for the active item, tab, or filter. Use an `accent` marker or label where the state needs extra clarity. |
| Separators | `divider`, `border`, `control-border` | Divider for groups, border for panel edges, control border for inputs or outlined controls whose edge indicates interactivity. |
| Text | `ink`, `body`, `muted`, `subtle` | Headings and key labels, long-form reading, secondary labels, and timestamps respectively. |
| Main action | `primary` on `on-primary` | Near-white button for the action that completes the current task. |
| Accent | `accent`, `accent-strong` | Focus outlines, links, active markers, and selected icon details. Use the stronger value on link hover or active press. |
| Feedback | `success`, `warning`, `danger` | Connection, sync, save, battery, reminder, and validation states. Pair each with a label or icon. |

The surface steps are intentionally small. Make the change in plane legible with a hairline or an inset before introducing a brighter fill. Most chrome is neutral graphite. Silver-violet belongs to interactive meaning, and semantic colors belong to status. Use a solid black scrim for overlays. Keep surface depth flat and let padding, borders, and contrast do the work.

## Typography

Use **Cormorant Garamond Medium** for editorial display with Georgia as the local fallback. It takes the role of Copernicus or Tiempos Headline in the reference. Use **Inter** for controls, metadata, and reading text, and **JetBrains Mono** for commands, addresses, fingerprints, coordinates, and playback time. Use tabular numerals for values that update in place. Keep Segoe UI Emoji ahead of bundled emoji subsets for Windows shaping.

| Style | Use |
| --- | --- |
| 64 px display | First-run pairing or another full-width, single-purpose introduction, where space permits. |
| 48 px display | Standard desktop workspace title. |
| 36 px display | Workspace title in smaller windows, or a feature panel heading such as the selected recording. |
| 28 px editorial title | Note, dialog, or selected-day heading. Use a single line with a tooltip where space is constrained. Long dialog titles can wrap. |
| 18 px title | List or card heading, contact name, settings group. |
| 16 px body | Messages and readable content. Notes may use Inter at 17 px with a 1.7 line height for sustained editing. |
| 14 px control | Navigation, buttons, fields, filter tabs, and dense list labels. |
| 13 px caption | Counts, dates, timestamps, and descriptive metadata. |

Use sentence case. Give priority to scale, whitespace, and family before increasing weight. A workspace has one prominent heading. Labels inside it describe the next level of the task, such as **Conversations**, **Devices**, or the name of the open note.

## Layout and spacing

Use the 4 px base grid. 8 and 12 px separate related controls, 16 and 24 px separate list groups, 24 and 32 px pad panels, and 48 px separates larger sections when they share a scrolling page. Intake or onboarding screens can use 64–96 px between sections.

| Layout | Behavior |
| --- | --- |
| Global bar | 64 px high. BlackBubbles wordmark and bubble mark at left, service navigation across the center, and the Mac connection at right. The connection opens Settings. |
| Workspace | Center within a 1280 px frame. Use 32 px horizontal padding on larger windows, 24 px at medium widths and 12–16 px at compact widths. |
| Masthead | Align the title with the content below. Place the current workspace's main action on the trailing edge when it fits. Allow it to wrap naturally below the title. Use 24–32 px before the title and 16–24 px after it. |
| Split workspace | Give the index 280–360 px and the working pane the rest. Use a 24 px gap at desktop. Align pane tops and give scrollable panes independent vertical space. |
| Reading column | Keep Notes and long text within roughly 65–75 characters per line when possible. Extra width belongs to the workbench margin, not wider lines. |
| Compact workspace | At roughly 760 px of available content width, show either the index or the open item. Put **Back** at the start of the detail toolbar and return focus to the originating row. |
| Compact navigation | When all service names stop fitting, switch the global bar to a labeled current-service menu. Keep every enabled service and Settings reachable from it. |
| Short windows | Reduce masthead and panel padding first. Allow lists, editors, and dialogs to scroll. Keep the current action and exit visible. |

Set breakpoints from usable content width. At 380 px, verify that the title, connection control, current service, and primary action can all be reached without horizontal page scrolling. Album, folder, and settings tab strips may scroll horizontally within their own region.

## Components and states

### Buttons, navigation, and selection

- **Primary** uses a near-white fill on black. Apply `primary-hover` and `primary-pressed` to interaction states. If two commands compete, give the pale fill to the one that commits or advances the current task. New Note can be primary on the index, with Save taking priority when an open note has unsaved edits.
- **Secondary** uses an elevated fill or an outline on the canvas. Use it for Refresh, Today, Cancel, export, and adjacent actions.
- **Icon controls** use 36–40 px targets with a visible focus ring, an accessible name, and a tooltip where the icon needs explanation. Use 44 px targets when the window is operated by touch.
- **Destructive** uses `danger` text on `danger-surface`. A destructive dialog has an explicit label such as **Move to Recently Deleted** and gives focus to a safe choice on opening.
- **Disabled** uses `disabled-surface` and `disabled-ink` and has no hover or pressed state. Keep explanatory text available when the reason affects the user's next action.
- **Navigation and filters** use `selected` for active or pressed items. Give active navigation an `aria-current` state, real tabs their selected state, and toggle filters an `aria-pressed` state. The current item's label remains legible at rest and on hover.

### Fields and editing

Use 40 px, 8 px-radius fields with a recessed fill. Labels remain visible when a value is typed. Keep search and editing input at the start of their workflow and keep Clear accessible by keyboard. On focus use a 2 px accent outline with enough offset to separate it from a light button or selected background. Show inline validation near the affected field.

Editing panes keep the title and save controls in a fixed utility bar and let the body scroll. Use clear **Edited**, **Saving…**, **Saved**, and error states. An error leaves the draft visible and the relevant next action available. When a note is locked, use a centered password form with the note name and a clear back action. Show password errors beside the form.

### Surfaces, menus, and feedback

- **Panels** use 12 px corners and a perimeter hairline when they touch another dark surface. Give secondary, dense content 16–24 px padding. Give a focal editor, player, or media surface 24–32 px where the viewport allows.
- **Lists** use 52–64 px rows for a title and preview. Align time or count on the trailing side. Use a solid hover step and a distinct selected background with an accent leading edge when it helps track the active item.
- **Menus and dialogs** use elevated surfaces, visible keyboard focus, and consistent 12 px corners. Menus align to their trigger. Dialogs fit the viewport, scroll internally when needed, and return focus to the launching control.
- **Status pills** pair a small dot or icon with text, such as **Saved** or **Offline**. A connection pill may show the paired Mac's name with a tooltip for a long name. Announce meaningful state changes, and keep toasts secondary to inline feedback.
- **Empty, loading, and offline states** occupy the space of the content they describe. Use a short title and a single useful instruction. Keep the main action nearby where action is possible.
- **Motion** uses 120–180 ms state transitions and 180–220 ms entrance transitions. Keep movement small and honor reduced motion. Playback and map controls remain usable without animation.

## Workspace patterns

| Workspace | Layout and signature detail |
| --- | --- |
| Messages | Conversation index and a recessed thread. Use graphite received bubbles and a subtle violet-tinted surface for sent messages. Keep Send as the bright action. Date dividers and attachment affordances are quieter than message text. |
| Notes | Folder filters and a searchable index lead into a spacious writing surface. The note title and save state sit above the reading column. Use the same pane for locked, unlocked, and editing states. |
| Photos | Give the album strip a simple baseline under the masthead. Photos fill a responsive grid with consistent gaps. Theater view uses a solid near-black backdrop and a compact tool strip that keeps Save As and Close discoverable. |
| Calendar | Put the month, Today, and month navigation above the grid. The agenda sits alongside the grid on wide windows and follows it on narrow windows. Mark today with an accent ring, use a separate selected-day fill, and preserve event color markers. |
| Reminders | Use a compact list and category index beside the selected list. Rows start with a clearly outlined checkbox. Keep the quick-add controls close to the list, and make completed and overdue states readable in text. |
| Voice Memos | Put search and recordings in a graphite list and give playback its own recessed panel. Use a small live progress line, elapsed and total time, and clearly separated Play, Stop, and Save As controls. On narrow windows, move the active player below the list. |
| Find My | Dock the device list beside the map on wide windows. On narrow windows, show the device list first and use a **Devices** return action from the map. Legible map controls, status labels, and a device detail card sit over the map. Give the selected pin a distinct shape or outline. |
| Settings | Start with one heading and section tabs for Connection, Services, Notifications, Autofill, and App. Pairing and sending-address help may sit side by side. Use recessed code blocks for commands and connection details, with selectable text and room for long values. |
| Pairing | Use the largest heading only if the first-run screen has room. Pairing gets one clear primary action, a visible label for the URI, and a concise state or error next to the form. |

## Acceptance checks

- Verify a full workspace at 1280 px, a compact layout around 800 px, and a narrow layout at 380 px. Include a short window, high text scaling, and keyboard-only navigation.
- Keep normal text and key placeholders at or above 4.5:1 contrast, large text at or above 3:1, and essential control boundaries and focus indicators at or above 3:1 against adjacent surfaces. Check hover and selected states as well as rest.
- Check that selection, disabled state, warning, and success are distinguishable without relying on hue alone. Preserve focus when opening or closing a menu, dialog, or compact detail.
- Check the same tokens on a real Notes editor, a list with data, a form, and a media surface. Shared controls should look related while each workspace's content determines its structure.
