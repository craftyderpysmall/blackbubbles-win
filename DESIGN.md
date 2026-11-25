---
version: alpha
name: BlackBubbles (Claude, dark)
description: |
  BlackBubbles is a dark-only desktop workspace for Apple services, styled after the
  Claude design system as getdesign.md presents it in dark mode. Neutral near-black
  surfaces carry white and grey text. Serif display headlines meet a humanist sans body,
  and a single warm coral accent marks the primary action, focus and selection. Depth
  comes from surface steps and hairlines, never from shadows, glows or gradients.

colors:
  # Surfaces: neutral near-blacks, lightest last.
  canvas: "#0c0c0e"
  deep: "#101012"
  card: "#141416"
  elevated: "#1c1c1f"
  hover: "#1c1c1f"
  divider: "#1f1f21"
  hairline: "#2a2a2b"
  hairline-strong: "#3a3a3c"
  control: "#66666a"
  # Text
  ink: "#ffffff"
  body: "#dbdbdb"
  muted: "#a7a7a8"
  muted-soft: "#8c8c8e"
  disabled: "#5e5e62"
  # Brand
  primary: "#cc785c"
  primary-active: "#a9583e"
  on-primary: "#ffffff"
  link: "#cc785c"
  # Companions and status
  teal: "#5db8a6"
  amber: "#e8a55a"
  success: "#5db872"
  warning: "#d4a017"
  danger: "#c64545"
  scrim: "rgba(12,12,14,0.72)"

typography:
  display-xl:
    fontFamily: "Cormorant Garamond, Georgia, serif"
    fontSize: 64px
    fontWeight: 500
    lineHeight: 1.05
    letterSpacing: -0.02em
  display-md:
    fontFamily: "Cormorant Garamond, Georgia, serif"
    fontSize: 36px
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: -0.015em
  title-lg:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 22px
    fontWeight: 500
    lineHeight: 1.3
  title-md:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 18px
    fontWeight: 500
    lineHeight: 1.4
  subtitle:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 18px
    fontWeight: 400
    lineHeight: 1.4
  reading:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 17px
    fontWeight: 400
    lineHeight: 1.7
  body-md:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 15px
    fontWeight: 400
    lineHeight: 1.5
  body-sm:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.55
  button:
    fontFamily: "Inter, Segoe UI, sans-serif"
    fontSize: 14px
    fontWeight: 500
    lineHeight: 1
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
  xs: 4px
  sm: 6px
  md: 8px
  lg: 12px
  xl: 16px
  pill: 9999px

spacing:
  xxs: 4px
  xs: 8px
  sm: 12px
  md: 16px
  lg: 24px
  xl: 32px
  xxl: 48px

components:
  top-nav:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.muted}"
    typography: "{typography.button}"
    height: 64px
  category-tab:
    backgroundColor: transparent
    textColor: "{colors.muted}"
    typography: "{typography.button}"
    rounded: "{rounded.md}"
    padding: 0 14px
    height: 36px
  category-tab-active:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
  hero-band:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink}"
    typography: "{typography.display-xl}"
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.button}"
    rounded: "{rounded.md}"
    padding: 0 20px
    height: 40px
  button-primary-active:
    backgroundColor: "{colors.primary-active}"
    textColor: "{colors.on-primary}"
  button-primary-disabled:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.disabled}"
  button-secondary:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.ink}"
    borderColor: "{colors.hairline}"
    rounded: "{rounded.md}"
    height: 40px
  button-outline:
    backgroundColor: transparent
    textColor: "{colors.ink}"
    borderColor: "{colors.hairline-strong}"
    rounded: "{rounded.md}"
    height: 40px
  button-quiet:
    backgroundColor: transparent
    textColor: "{colors.muted}"
    rounded: "{rounded.md}"
  button-danger:
    backgroundColor: transparent
    textColor: "{colors.danger}"
    borderColor: "{colors.danger}"
    rounded: "{rounded.md}"
  button-icon-circular:
    backgroundColor: transparent
    textColor: "{colors.muted}"
    rounded: "{rounded.pill}"
    size: 36px
  text-input:
    backgroundColor: "{colors.deep}"
    textColor: "{colors.ink}"
    borderColor: "{colors.control}"
    typography: "{typography.body-sm}"
    rounded: "{rounded.md}"
    padding: 0 14px
    height: 40px
  text-input-focused:
    borderColor: "{colors.primary}"
    ring: "3px {colors.primary} at 15%"
  index-card:
    backgroundColor: "{colors.card}"
    borderColor: "{colors.hairline}"
    rounded: "{rounded.lg}"
  focal-pane:
    backgroundColor: "{colors.deep}"
    borderColor: "{colors.hairline}"
    rounded: "{rounded.lg}"
  list-row:
    backgroundColor: transparent
    borderColor: "{colors.hairline}"
    height: 64px
    padding: 0 16px
  list-row-active:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.ink}"
    marker: "2px {colors.primary} leading edge"
  pane-header:
    backgroundColor: transparent
    textColor: "{colors.ink}"
    typography: "{typography.title-md}"
    minHeight: 64px
  code-window-card:
    backgroundColor: "{colors.deep}"
    textColor: "{colors.body}"
    typography: "{typography.code}"
    rounded: "{rounded.lg}"
  code-tab:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.muted}"
    typography: "{typography.code}"
    rounded: "{rounded.sm}"
  badge-pill:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.body}"
    borderColor: "{colors.hairline}"
    rounded: "{rounded.pill}"
    padding: 2px 10px
  menu:
    backgroundColor: "{colors.elevated}"
    borderColor: "{colors.hairline-strong}"
    rounded: "{rounded.lg}"
    padding: 6px
  dialog:
    backgroundColor: "{colors.elevated}"
    borderColor: "{colors.hairline-strong}"
    rounded: "{rounded.lg}"
    padding: 32px
    width: 480px
  toast:
    backgroundColor: "{colors.elevated}"
    textColor: "{colors.ink}"
    borderColor: "{colors.hairline-strong}"
    rounded: "{rounded.lg}"
  bubble-received:
    backgroundColor: "{colors.card}"
    borderColor: "{colors.hairline-strong}"
    rounded: "{rounded.lg}"
  bubble-sent:
    backgroundColor: "{colors.primary} at 14%"
    borderColor: "{colors.primary} at 30%"
    rounded: "{rounded.lg}"
  bubble-sent-sms:
    backgroundColor: "{colors.success} at 8%"
    borderColor: "{colors.success} at 25%"
    rounded: "{rounded.lg}"
---

## Overview

BlackBubbles brings the Claude design system to a dark desktop workspace. The canvas is a **neutral near-black** (`{colors.canvas}`, #0c0c0e), cool and quiet rather than the warm browns of Claude's light-page footers. Headlines are a **serif display face** at weight 500 with tight tracking. Everything you operate or read is set in a **humanist sans**. A single **coral** (`{colors.primary}`, #cc785c) carries the brand: the one primary action in each workspace, the focus ring, links, and the edge of the selected row.

The look is editorial and calm. Each workspace opens with a large serif headline and a live sub-line, then gives the rest of the window to the work: a list of conversations, notes or recordings beside a focused working pane. Depth comes from small steps between near-black surfaces and from 1px hairlines. There are no drop shadows, glows, gradients or tinted surfaces.

The palette derives from getdesign.md's analysis of Claude (cream canvas, coral CTA, Copernicus serif, StyreneB sans). The dark values match getdesign.md's own dark presentation of that analysis: background #0c0c0e, hairline #2a2a2b, white headings, #dbdbdb body and #a7a7a8 secondary text. The light palette remains only as a reference at the end of this file.

**Key characteristics**
- Neutral near-black canvas with white headings and soft grey body text.
- Serif display (Cormorant Garamond 500) for workspace headlines; Inter for every control and for reading; JetBrains Mono for code and fixed-width detail.
- Coral is scarce: one coral button per workspace, plus coral for focus, links and active markers.
- Surfaces step up in small increments (canvas → deep → card → elevated) and are separated by hairlines.
- Hierarchical radii: 8px controls, 12px panels and menus, pill for badges and icon buttons.

## Colors

### Surfaces

| Token | Value | Use |
| --- | --- | --- |
| `{colors.canvas}` | #0c0c0e | App floor, top nav and hero band. |
| `{colors.deep}` | #101012 | The focal working pane (thread, note, player, map, calendar grid) and text inputs. |
| `{colors.card}` | #141416 | Index panes (conversation, note, recording and device lists), settings cards, and the code block inside a code window. |
| `{colors.elevated}` | #1c1c1f | Menus, dialogs, toasts, secondary buttons, the active tab or filter, and the selected row. |
| `{colors.hover}` | #1c1c1f | The hover step on interactive rows and quiet buttons. |
| `{colors.divider}` | #1f1f21 | The faintest in-pane separators. |
| `{colors.hairline}` | #2a2a2b | Nav rule, card and pane edges, row separators and outline buttons. |
| `{colors.hairline-strong}` | #3a3a3c | Menu, dialog and toast edges, received bubbles, and badges over photos. |
| `{colors.control}` | #66666a | Boundaries of inputs, checkboxes, switches and other controls (3:1 on the canvas). |

Surfaces get lighter as they come forward, but each step is small. Use a hairline to make a step legible before reaching for a lighter fill.

### Text

| Token | Value | Use |
| --- | --- | --- |
| `{colors.ink}` | #ffffff | Headlines, titles, labels and active navigation. |
| `{colors.body}` | #dbdbdb | Running text: messages, notes and descriptions. |
| `{colors.muted}` | #a7a7a8 | Secondary labels, inactive navigation, sub-lines and descriptions under settings. |
| `{colors.muted-soft}` | #8c8c8e | Captions, timestamps, counts, placeholders and out-of-month dates. The lowest tone allowed for readable text. |
| `{colors.disabled}` | #5e5e62 | Disabled foregrounds only. |

### Brand

- **Coral** (`{colors.primary}`, #cc785c): the primary button, the focus outline, inline links, the 2px leading edge of the selected row, the unread dot, the text caret, today's date, a completed reminder's check, the playing recording's control and the selected map pin.
- **Coral active** (`{colors.primary-active}`, #a9583e): the pressed primary button. Coral doesn't change on hover.
- **On primary** (`{colors.on-primary}`, #ffffff): labels and icons on coral.

### Companions and status

- **Teal** (#5db8a6) and **amber** (#e8a55a): user-data colors for calendars and reminder lists, alongside coral and the status colors. An Apple calendar color maps to the nearest of these by hue, and near-greys map to `{colors.muted}`.
- **Success** (#5db872): connected, saved, playing.
- **Warning** (#d4a017): connecting, edited, a chat that sends from email.
- **Danger** (#c64545): offline, errors, overdue, destructive actions.

Pair every status color with a label, icon or shape, never color alone.

### Translucent uses

A few roles use these colors at reduced opacity: sent iMessage bubbles (coral at 14% with a 30% edge), sent SMS bubbles (success at 8% with a 25% edge), the focused-input ring (coral at 15%), text selection (coral at 38%) and the dialog scrim (canvas at 72%). Introduce no other hues.

## Typography

### Families
- **Cormorant Garamond** (bundled, OFL) stands in for Copernicus / Tiempos Headline. It is used at weight 500, never bold, with negative tracking.
- **Inter** (bundled) stands in for StyreneB. It is used for navigation, buttons, labels, lists, messages, notes and settings.
- **JetBrains Mono** (bundled) is used for commands, the pinned Mac address, the Shortcut header, coordinates and playback time.
- Keep **Segoe UI Emoji** ahead of the bundled Noto emoji subsets so Windows shapes ZWJ sequences.

### Scale

| Token | Size | Weight | Line height | Use |
| --- | --- | --- | --- | --- |
| `{typography.display-xl}` | 40–64px, fluid | 500 | 1.05 | The workspace headline. Calendar uses the month name. Drops to 36px in windows shorter than 620px. |
| `{typography.display-md}` | 28–36px | 500 | 1.15 | Feature-panel headings: the playing recording, the locked note. |
| — | 34–56px | 500 | ~0.9 | Display numerals: the selected calendar day and reminder category counts. |
| `{typography.title-lg}` | 22px | 500 | 1.3 | Settings card titles. |
| `{typography.title-md}` | 18px | 500 | 1.4 | Pane titles, dialog titles and empty-state titles. |
| `{typography.subtitle}` | 18px | 400 | 1.4 | The sub-line under a workspace headline, in `{colors.muted}`. |
| `{typography.reading}` | 17px | 400 | 1.7 | The note editor. |
| `{typography.body-md}` | 15px | 400 | 1.5 | Message bubbles and the composer. |
| `{typography.body-sm}` | 14px | 400 | 1.55 | Default UI text, row titles and inputs. |
| `{typography.button}` | 14px | 500 | 1 | Buttons, navigation and tabs. Small buttons and filters use 13px. |
| `{typography.caption}` | 13px | 400 | 1.4 | Previews, metadata and settings descriptions. Times use 12px with tabular numerals. |
| `{typography.code}` | 13px | 400 | 1.6 | Code windows and fixed-width values. |

### Principles
- Prefer a larger serif over a heavier weight for emphasis. Display type is never bold.
- Use the serif only for display: workspace headlines, feature-panel headings and display numerals. Pane titles, card titles and dialogs use Inter.
- Use sentence case. Use tabular numerals for values that update in place: times, counts, battery, playback.

## Layout

### Spacing
The base unit is 4px. Use 8–12px between related controls, 16–24px between groups, 24–32px inside panels, and 48px between sections that share a scrolling page.

### Frame

| Region | Behavior |
| --- | --- |
| Top nav | 64px on `{colors.canvas}` with a `{colors.hairline}` bottom rule. The coral bubble mark and serif "BlackBubbles" wordmark sit at left. Service tabs are centered. The right side holds the Mac connection badge (status dot plus Mac name), then a circular Settings button. |
| Workspace | Centered in a 1200px frame with 16px side padding at compact widths, 24px at medium and 32px at wide. |
| Hero band | The serif headline, the sub-line with live context (unread count, notes in folder, month summary, open reminders, device freshness) and the workspace's actions on the trailing edge. The band sits on the flat canvas. Allow 16–32px above and 24–32px below. |
| Split workspace | The index card is 280–360px and the focal pane takes the rest, with a 16–24px gap. Pane tops align, and each pane scrolls on its own. |
| Reading column | The note editor keeps a ~68-character column centered in the focal pane. Extra width becomes margin, not longer lines. |

### Responsive

| Condition | Change |
| --- | --- |
| Header width under ~1040px | Service tabs collapse into an outline "current service" menu listing every shown service and Settings. |
| Header width under 600px | The wordmark text hides; the bubble mark stays. |
| Content width under 760px | Split workspaces show either the index or the open item. Detail panes start with **Back**, which returns focus to the originating row. |
| Content width under 980px | Calendar stacks the agenda below the month grid. |
| Window height under 620px | The headline shrinks to 36px, the sub-line to 15px, and workspace insets tighten to 12px. |

At 380px wide, the headline, connection badge, current service and primary action must all be reachable without horizontal page scrolling. Filter strips and settings tabs may scroll horizontally within their own row.

## Elevation and depth

| Level | Treatment | Use |
| --- | --- | --- |
| Flat | `{colors.canvas}`, no border | Nav, hero band. |
| Index | `{colors.card}` + 1px `{colors.hairline}` | Lists and settings cards. |
| Focal | `{colors.deep}` + 1px `{colors.hairline}` | The working pane, code windows. |
| Raised | `{colors.elevated}` + 1px `{colors.hairline-strong}` | Menus, dialogs, toasts, map overlays. |
| Overlay | `{colors.scrim}` behind a raised surface | Dialogs; the photo viewer uses a solid `{colors.canvas}` backdrop. |

There are no drop shadows, glows or gradients anywhere.

## Shapes

| Token | Value | Use |
| --- | --- | --- |
| `{rounded.xs}` | 4px | Tiny inline marks. |
| `{rounded.sm}` | 6px | Menu items, code tabs, segmented-control segments. |
| `{rounded.md}` | 8px | Buttons, inputs, category tabs, list tiles and photo tiles. |
| `{rounded.lg}` | 12px | Cards, panes, menus, dialogs, toasts, bubbles and code windows. |
| `{rounded.xl}` | 16px | Reserved for a large marquee surface. |
| `{rounded.pill}` | 9999px | Badges, status dots, avatars, icon buttons and the Play control. |

## Components

### Navigation
- **Top nav tabs** (`category-tab`): 36px, 14px/500 Inter in `{colors.muted}`. The active tab gets an `{colors.elevated}` fill, `{colors.ink}` text and `aria-current="page"`.
- **Keyboard:** Ctrl+Tab and Ctrl+Shift+Tab cycle through the shown services in nav order, wrapping, and work from any focused field.
- **Connection badge:** a `badge-pill` with a status dot (success, warning, danger or muted-soft) and the Mac's name. It opens Settings.

### Buttons
- **Primary:** coral with a white label, 40px, 8px radius, 20px horizontal padding. It darkens to `{colors.primary-active}` on press. Use one per workspace, for the action that completes the current task (New note, Save on an edited note, New item, Pair, Add, Send).
- **Secondary:** `{colors.elevated}` with a hairline, for adjacent actions such as Refresh, Today or Hide devices.
- **Outline:** transparent with `{colors.hairline-strong}`, for tertiary actions such as Save As, month arrows or Disable endpoint.
- **Quiet:** text-only in `{colors.muted}`, filling on hover. Use it for icon controls and Cancel.
- **Danger:** a `{colors.danger}` label and border on a transparent fill. A destructive dialog names the action explicitly ("Move to Recently Deleted") and focuses the safe choice.
- **Icon buttons:** 36px circles with an accessible name and a tooltip.
- **Disabled:** `{colors.elevated}` fill, `{colors.disabled}` label, no hover.

### Inputs
- **Text input:** 40px, `{colors.deep}` fill, `{colors.control}` border, 8px radius, `{colors.muted-soft}` placeholder. On focus, the border turns coral with a 3px coral ring at 15%. Search fields lead with a magnifier and end with a clear button.
- **Switch:** 36×20 track with a `{colors.control}` border. When on, the track is coral with a white thumb.
- **Checkbox (reminders):** an 18px circle with a `{colors.control}` border. When done, it fills coral with a white check.
- **Segmented control:** an 8px `{colors.deep}` well with a hairline; the active segment takes `{colors.elevated}`.

### Surfaces
- **Index card:** a `{colors.card}` panel with a search or filter header separated by a hairline, then full-bleed rows.
- **List row:** 64–68px for a title and preview, 16px horizontal padding, a hairline between rows, trailing time in `{colors.muted-soft}`. Hover moves to `{colors.hover}`. The active row moves to `{colors.elevated}` with a 2px coral leading edge.
- **Pane header:** at least 64px, an 18px/500 title with an optional caption, and the pane's controls on the trailing side, above a hairline.
- **Code window:** a `{colors.deep}` shell with a 40px chrome strip (danger, warning and success dots plus a mono `code-tab`), then the code on a `{colors.card}` block.
- **Badge:** a pill on `{colors.elevated}` with a hairline, 12px text, and an optional status dot or icon.
- **Menus and dialogs:** `{colors.elevated}` with `{colors.hairline-strong}`, 12px radius and visible keyboard focus. Menus align to their trigger. Dialogs fit the viewport, scroll internally and return focus to their opener.
- **Toasts:** bottom-right, `{colors.elevated}`, secondary to inline feedback.
- **Empty states:** a small icon in `{colors.muted-soft}`, an 18px title and one instruction, centered in the space of the missing content.

### Motion
Use 150ms for state changes and 180–200ms for entrances (a 4px rise or a 0.98 scale). Honor reduced motion; playback and map controls work without animation.

## Workspace patterns

| Workspace | Pattern |
| --- | --- |
| Messages | The conversation index beside a `{colors.deep}` thread. Received bubbles are cards; sent bubbles take the coral wash (SMS: success wash). Day labels sit between hairlines. The composer pairs an attach button with a coral Send. Unread chats show a coral dot. |
| Notes | Folder tabs and a searchable index lead to the reading column on the focal pane, with Edited / Saving… / Saved status and Save in the pane header. Locked notes show a centered password form with a 36px serif heading. |
| Photos | Album tabs under a hairline, and a responsive grid of 8px-radius tiles with 12px gaps. The viewer is a solid canvas with a 64px tool strip holding Save As (primary) and Close. |
| Calendar | The month is the headline, with Today, the month arrows and New item beside it. The grid draws 1px hairline gaps. Today gets a coral circle, the selected day `{colors.elevated}`. The agenda opens with the selected day as a large serif numeral. |
| Reminders | Category tiles with serif counts, colored-dot lists and a selected list with grouped rows. Quick add sits at the bottom of the pane. |
| Voice Memos | A recording index beside a player panel: status badge, serif title, a 2px progress line with mono times, a circular coral Play, Stop, and Save As. |
| Find My | The device list beside a dark map. The selected pin is a coral square; others are dark circles. A raised device card shows battery, accuracy and coordinates. |
| Settings | Category tabs for Connection, Services, Notifications, Autofill and App; 32px-padded cards with 22px titles; commands and connection details in code windows. Before pairing, the headline reads "Pair with your Mac". |

## Do's and don'ts

### Do
- Keep every surface a neutral near-black from the table above.
- Give each workspace one serif headline and one coral action.
- Separate surfaces with hairlines and small fill steps.
- Keep readable text selectable and focus visible, and return focus when a compact detail pane closes.

### Don't
- Don't use warm-brown or cream surfaces in the app.
- Don't add glows, gradients, drop shadows or tinted panels.
- Don't paint coral onto large areas or more than one button per workspace.
- Don't bold the serif or set controls in it.
- Don't rely on color alone for status.

## Accessibility checks

- Verify each workspace at 1280px, around 800px and at 380px, in a window under 620px tall, at high text scaling, and with the keyboard only.
- Keep normal text at 4.5:1 or better. `{colors.muted-soft}` on `{colors.card}` is about 5.4:1.
- **Known caveats:** white on coral is about 3.2:1 and `{colors.danger}` on the canvas about 4.1:1, both under 4.5:1 for small text. Keep coral labels at 14px/500 or larger, and pair danger text with an icon or border.
- Control boundaries and focus indicators must reach 3:1 against adjacent surfaces.

## Reference: Claude light palette (not used)

getdesign.md's source analysis describes Claude's light marketing site. It is kept here for provenance only: canvas #faf9f5, surface-soft #f5f0e8, surface-card #efe9de, surface-cream-strong #e8e0d2, ink #141413, body #3d3d3a, muted #6c6a64, hairline #e6dfd8, and the dark product surfaces surface-dark #181715, surface-dark-soft #1f1e1b and surface-dark-elevated #252320. Its display face is Copernicus (substitutes: Tiempos Headline, Cormorant Garamond, EB Garamond) and its sans is StyreneB (substitute: Inter).
