import { useEffect, useRef, useState, type ReactNode } from "react";
import { Tabs } from "radix-ui";
import { AudioLines, Bell, Calendar, Check, Image, Info, Keyboard, LayoutGrid, Link2, ListChecks, Mail, MapPin, MessageCircle, NotebookPen, Phone, Plus, Power, Trash2, type LucideIcon } from "lucide-react";
import { errorText, native, type ConfigView, type Notifications, type Services } from "@/lib/bb";
import { useApp } from "@/lib/app";
import { cn } from "@/lib/utils";
import { Badge, Button, Card, Input, Switch, WorkspaceHeading } from "@/components/ui";

function Section({ icon: Icon, title, description, children }: { icon: LucideIcon; title: string; description?: string; children: ReactNode }) {
  return (
    <Card className="p-4 sm:p-6">
      <div className="flex items-center gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-control border border-border-strong bg-raised">
          <Icon className="size-4 text-fg" />
        </span>
        <h2 className="text-lg font-medium text-fg">{title}</h2>
      </div>
      {description && <p className="mt-2 text-sm text-muted">{description}</p>}
      <div className="mt-3">{children}</div>
    </Card>
  );
}

function Group({ title, children, off }: { title: string; children: ReactNode; off?: boolean }) {
  return (
    <div className={cn("mt-3 first:mt-0", off && "pointer-events-none opacity-40")} title={off ? `${title} is hidden in Services` : undefined}>
      <div className="mb-0.5 px-1 text-[11px] font-medium tracking-wide text-subtle uppercase">{title}</div>
      <div className="divide-y divide-border">{children}</div>
    </div>
  );
}

function Nested({ children, disabled }: { children: ReactNode; disabled?: boolean }) {
  return <div className={cn("ml-4 border-l border-border pl-3", disabled && "pointer-events-none opacity-40")}>{children}</div>;
}

function NumberField({ label, suffix, value, min, max, onChange, disabled }: { label: string; suffix: string; value: number; min: number; max: number; onChange: (v: number) => void; disabled?: boolean }) {
  return (
    <label className={cn("flex items-center justify-between gap-4 px-1 py-2", disabled && "opacity-40")}>
      <span className="text-[13px] text-fg">{label}</span>
      <span className="flex items-center gap-2">
        <Input
          type="number"
          min={min}
          max={max}
          value={value}
          disabled={disabled}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (Number.isFinite(v)) onChange(Math.min(max, Math.max(min, Math.round(v))));
          }}
          className="h-8 w-20 text-right"
        />
        <span className="w-6 text-xs text-subtle">{suffix}</span>
      </span>
    </label>
  );
}

function Code({ children }: { children: ReactNode }) {
  return <code className="selectable block overflow-x-auto rounded-control border border-border bg-deep px-4 py-3 font-mono text-[13px] whitespace-pre text-body">{children}</code>;
}

export default function SettingsPage() {
  const { boot, config } = useApp();
  const sectionScroll = useRef<HTMLDivElement>(null);
  return (
    <section className="flex h-full flex-col">
      <WorkspaceHeading title="Settings" />
      <Tabs.Root defaultValue="connection" onValueChange={() => sectionScroll.current?.scrollTo(0, 0)} className="flex min-h-0 flex-1 flex-col gap-4 sm:gap-6">
        <Tabs.List aria-label="Settings sections" className="flex shrink-0 gap-1 overflow-x-auto border-b border-border pb-3">
          {[
            ["connection", "Connection"],
            ["services", "Services"],
            ["notifications", "Notifications"],
            ["autofill", "Autofill"],
            ["app", "App"],
          ].map(([id, label]) => (
            <Tabs.Trigger
              key={id}
              value={id}
              className="h-9 shrink-0 rounded-control px-3 text-sm font-medium text-muted hover:bg-hover data-[state=active]:bg-selected data-[state=active]:text-fg"
            >
              {label}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        <div ref={sectionScroll} className="min-h-0 flex-1 overflow-y-auto">
          <Tabs.Content value="connection" forceMount className="grid max-w-[1120px] gap-4 data-[state=inactive]:hidden lg:grid-cols-[1.2fr_1fr] lg:items-start">
            <Connection />
            <SendingAddress />
          </Tabs.Content>
          <Tabs.Content value="services" forceMount className="max-w-3xl data-[state=inactive]:hidden">
            <ServiceSettings />
          </Tabs.Content>
          <Tabs.Content value="notifications" forceMount className="max-w-3xl data-[state=inactive]:hidden">
            <NotificationSettings />
          </Tabs.Content>
          <Tabs.Content value="autofill" forceMount className="max-w-3xl data-[state=inactive]:hidden">
            <AutofillSettings />
          </Tabs.Content>
          <Tabs.Content value="app" forceMount className="grid max-w-[1120px] gap-4 data-[state=inactive]:hidden lg:grid-cols-2 lg:items-start">
            <StartupSettings />
            <Section icon={Info} title="About">
              <div className="selectable space-y-1 break-words text-sm text-muted">
                <div>BlackBubbles {boot.version}</div>
                <div className="text-subtle">Settings file: {boot.config_path}</div>
                <div className="text-subtle">
                  {boot.primary ? "This app handles autofill and notifications, also while it's in the tray." : "Another BlackBubbles process handles autofill and notifications."}
                </div>
                {!config.server && <div className="text-subtle">Not paired yet.</div>}
              </div>
            </Section>
          </Tabs.Content>
        </div>
      </Tabs.Root>
    </section>
  );
}

function useSaver() {
  const { setConfig, toast } = useApp();
  return (p: Promise<ConfigView>, done?: string) =>
    p.then(
      (c) => {
        setConfig(c);
        if (done) toast(done);
      },
      (e) => toast(errorText(e)),
    );
}

function Connection() {
  const { boot, config, status, setConfig, toast } = useApp();
  const [uri, setUri] = useState("");
  const [name, setName] = useState(boot.device_name);
  const [busy, setBusy] = useState(false);
  const [pairError, setPairError] = useState<string | null>(null);
  const [text, dot] =
    status.state === "Connected"
      ? [`Connected to ${status.detail.server_name}`, "var(--color-success)"]
      : status.state === "Connecting"
        ? ["Connecting…", "var(--color-warning)"]
        : status.state === "Disconnected"
          ? [`Offline: ${status.detail}`, "var(--color-danger)"]
          : ["Not paired", "var(--color-subtle)"];

  const pair = async () => {
    if (busy || !uri.trim().startsWith("blackbubbles://")) return;
    setBusy(true);
    setPairError(null);
    try {
      setConfig(await native.pair(uri.trim(), name));
      setUri("");
      toast("Paired with your Mac");
    } catch (error) {
      setPairError(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section icon={Link2} title={config.server ? "Mac connection" : "Pair with your Mac"}>
      <div className="flex flex-wrap items-center gap-2">
        <Badge dot={dot} title={text}>{text}</Badge>
      </div>
      {config.server && (
        <div className="mt-3">
          <Code>{config.server.host}:{config.server.port} · pinned {config.server.fingerprint.slice(0, 12)}…</Code>
        </div>
      )}
      <div className="mt-4 space-y-2">
        <p className="text-sm text-muted" id="pairing-help">Run this command on your Mac and paste the pairing URI below.</p>
        <Code>bbd pair --host &lt;Mac hostname or Tailscale IP&gt;</Code>
      </div>
      <form className="mt-4 grid gap-3" onSubmit={(event) => { event.preventDefault(); void pair(); }}>
        <label className="grid gap-1.5 text-sm text-body">
          Pairing URI
          <Input
            placeholder="blackbubbles://…"
            value={uri}
            onChange={(e) => setUri(e.target.value)}
            aria-describedby={pairError ? "pairing-help pairing-error" : "pairing-help"}
            aria-invalid={pairError ? true : undefined}
            className="font-mono text-[13px]"
          />
        </label>
        <div className="flex flex-wrap items-end gap-3">
          <label className="grid min-w-40 flex-1 gap-1.5 text-sm text-body">
            Windows device name
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <Button type="submit" variant="primary" icon={Link2} busy={busy} disabled={!uri.trim().startsWith("blackbubbles://")}>
            {busy ? "Pairing…" : "Pair"}
          </Button>
        </div>
        {pairError && <p id="pairing-error" role="alert" className="text-sm text-danger">{pairError}</p>}
      </form>
    </Section>
  );
}

function SendingAddress() {
  return (
    <Section icon={Phone} title="Sending address" description="Messages go out from the address your Mac uses for each conversation. BlackBubbles never picks email on its own.">
      <div className="text-[13px] text-fg">To send from your phone number:</div>
      <ul className="mt-2 space-y-2">
        {[
          "On your iPhone: Settings → Messages → Send & Receive → under “Start New Conversations From”, choose your phone number.",
          "On the Mac: Messages → Settings → iMessage → make sure your phone number is ticked, and set “Start new conversations from” to it.",
          "Conversations that already use your email keep using it. Each chat header shows the address (a yellow dot marks email); start a new conversation to switch it.",
        ].map((step) => (
          <li key={step} className="flex gap-2 text-xs text-muted">
            <Check className="mt-0.5 size-3.5 shrink-0 text-muted" />
            <span>{step}</span>
          </li>
        ))}
      </ul>
    </Section>
  );
}

const SERVICES: [keyof Services, LucideIcon, string, string][] = [
  ["messages", MessageCircle, "Messages", "iMessage and SMS"],
  ["photos", Image, "Photos", "Library and albums"],
  ["notes", NotebookPen, "Notes", ""],
  ["calendar", Calendar, "Calendar", "Events (and reminders with a date, if Reminders is on)"],
  ["reminders", ListChecks, "Reminders", "The Reminders app's lists"],
  ["voice_memos", AudioLines, "Voice Memos", ""],
  ["find_my", MapPin, "Find My", "Your Apple devices"],
];

function ServiceSettings() {
  const { config } = useApp();
  const save = useSaver();
  const v = config.services;
  return (
    <Section icon={LayoutGrid} title="Services" description="Choose which Apple services BlackBubbles shows. A hidden service gets no tab and no notifications. Autofill keeps working either way.">
      <div className="divide-y divide-border">
        {SERVICES.map(([key, Icon, name, hint]) => (
          <div key={key} className="flex items-center gap-3">
            <Icon className="size-4 shrink-0 text-muted" />
            <div className="min-w-0 flex-1">
              <Switch checked={v[key]} onChange={(on) => void save(native.saveServices({ ...v, [key]: on }))} label={name} description={hint || undefined} />
              {key === "find_my" && (
                <Nested disabled={!v.find_my}>
                  <Switch
                    checked={v.find_my_family}
                    onChange={(on) => void save(native.saveServices({ ...v, find_my_family: on }))}
                    label="Show family members' devices"
                    description="Devices of people in your Family Sharing group"
                  />
                </Nested>
              )}
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}

function NotificationSettings() {
  const { config } = useApp();
  const on = config.services;
  const save = useSaver();
  const n = config.notifications;
  const set = (patch: Partial<Notifications>) => void save(native.saveNotifications({ ...n, ...patch }));
  const sw = (key: keyof Notifications, label: string, description?: string, disabled?: boolean) => (
    <Switch checked={n[key] as boolean} onChange={(v) => set({ [key]: v })} label={label} description={description} disabled={disabled} />
  );

  return (
    <Section icon={Bell} title="Notifications" description="Choose exactly what shows up as a desktop notification.">
      {sw("enabled", "Desktop notifications", "Turn everything off with one switch")}
      <div className={cn(!n.enabled && "pointer-events-none opacity-40")}>
        <Group title="Messages" off={!on.messages}>
          {sw("messages", "New messages")}
          <Nested disabled={!n.messages}>
            {sw("imessage", "iMessage")}
            {sw("sms", "SMS / RCS")}
            {sw("show_sender", "Show sender", "Off: notifications just say BlackBubbles")}
            {sw("show_preview", "Show message preview", "Off: “New message” instead of the text")}
            {sw("skip_codes", "Skip verification-code texts", "Autofill already shows those")}
            {sw("only_when_unfocused", "Only when BlackBubbles isn't focused")}
            <div className="flex items-center justify-between gap-2 px-1 py-2 text-xs text-subtle">
              {n.muted_chats.length ? (
                <>
                  <span>{n.muted_chats.length} muted conversation{n.muted_chats.length > 1 ? "s" : ""}</span>
                  <Button size="sm" onClick={() => set({ muted_chats: [] })}>Unmute all</Button>
                </>
              ) : (
                <span className="flex items-center gap-1.5"><Bell className="size-3.5" /> Mute a conversation with the bell in its header.</span>
              )}
            </div>
          </Nested>
        </Group>
        <Group title="Calendar" off={!on.calendar}>
          {sw("calendar_reminders", "Event reminders")}
          <Nested disabled={!n.calendar_reminders}>
            <NumberField label="Remind me before events start" suffix="min" value={n.reminder_minutes} min={0} max={240} onChange={(v) => set({ reminder_minutes: v })} />
          </Nested>
        </Group>
        <Group title="Reminders" off={!on.reminders}>
          {sw("reminders_due", "Reminders when they're due", "At their time; date-only ones at 09:00")}
        </Group>
        <Group title="Find My" off={!on.find_my}>
          {sw("low_battery", "Low battery on a device")}
          <Nested disabled={!n.low_battery}>
            <NumberField label="Below" suffix="%" value={n.low_battery_percent} min={5} max={50} onChange={(v) => set({ low_battery_percent: v })} />
          </Nested>
        </Group>
        <Group title="Library">
          {sw("photos", "New photos", undefined, !on.photos)}
          {sw("notes", "Notes changed on the Mac", undefined, !on.notes)}
          {sw("voice_memos", "New voice memos", undefined, !on.voice_memos)}
        </Group>
        <Group title="System">
          {sw("connection", "Connection lost / restored", "After the Mac has been unreachable for 90 s")}
          {sw("quiet_hours", "Quiet hours", "Nothing is shown during this window")}
          <Nested disabled={!n.quiet_hours}>
            <div className="flex flex-wrap items-center justify-between gap-2 px-1 py-2">
              <span className="text-[13px] text-fg">From</span>
              <span className="flex items-center gap-2">
                <Input type="time" aria-label="Quiet hours start" value={n.quiet_start} onChange={(e) => e.target.value && set({ quiet_start: e.target.value })} className="h-8 w-24" />
                <span className="text-xs text-subtle">to</span>
                <Input type="time" aria-label="Quiet hours end" value={n.quiet_end} onChange={(e) => e.target.value && set({ quiet_end: e.target.value })} className="h-8 w-24" />
              </span>
            </div>
          </Nested>
        </Group>
      </div>
      <p className="mt-3 text-xs text-subtle">Shown while BlackBubbles runs, including when the window is closed to the tray.</p>
    </Section>
  );
}

function StartupSettings() {
  const { toast } = useApp();
  const [autostart, setAutostart] = useState<boolean | null>(null);
  useEffect(() => {
    native.getAutostart().then(setAutostart, (e) => toast(errorText(e)));
  }, [toast]);
  const change = (v: boolean) =>
    native.setAutostart(v).then(setAutostart, (e) => toast(`Couldn't change Start at login: ${errorText(e)}`));
  return (
    <Section icon={Power} title="Startup" description="Closing the window keeps BlackBubbles in the tray, so codes and notifications still arrive. Quit it from the tray icon's menu.">
      <div className="divide-y divide-border">
        <Switch checked={!!autostart} disabled={autostart === null} onChange={(v) => void change(v)} label="Start at login" description="Starts in the tray when you sign in to Windows" />
      </div>
    </Section>
  );
}

function AutofillSettings() {
  const { config } = useApp();
  const save = useSaver();
  const a = config.autofill;
  const [address, setAddress] = useState("");
  const [password, setPassword] = useState("");
  const set = (patch: Partial<typeof a>) => void save(native.saveAutofill({ enabled: a.enabled, from_server: a.from_server, notify: a.notify, clear_after: a.clear_after, ...patch }));

  return (
    <Section icon={Keyboard} title="Autofill" description="Verification codes and links from texts and email go straight to your clipboard.">
      <div className="divide-y divide-border">
        <Switch checked={a.enabled} onChange={(v) => set({ enabled: v })} label="Enabled" />
        <Switch checked={a.from_server} onChange={(v) => set({ from_server: v })} label="Codes from iMessage / SMS on the Mac" />
        <Switch checked={a.notify} onChange={(v) => set({ notify: v })} label="Click-to-paste notification" description="Off: codes are copied silently" />
        <NumberField label="Restore the clipboard after" suffix="s" value={a.clear_after} min={5} max={600} onChange={(v) => set({ clear_after: v })} />
      </div>
      <Group title="iPhone Shortcut endpoint">
        <div className="space-y-2 px-1 py-2">
          {a.http ? (
            <>
              <div className="text-xs text-muted">POST http://&lt;this computer&gt;.local:{a.http.port}/message</div>
              <Code>Authorization: Bearer {a.http.token}</Code>
              <div className="text-xs text-subtle">The first time, Windows Firewall asks whether BlackBubbles may accept connections: allow <span className="text-muted">private networks</span> only.</div>
              <Button variant="outline" size="sm" onClick={() => void save(native.autofillHttp(false))}>Disable endpoint</Button>
            </>
          ) : (
            <>
              <div className="text-xs text-subtle">Works even when the Mac is offline.</div>
              <Button variant="outline" size="sm" icon={Plus} onClick={() => void save(native.autofillHttp(true))}>Enable with a new token</Button>
            </>
          )}
        </div>
      </Group>
      <Group title="Gmail">
        <div className="space-y-2 px-1 py-2">
          {a.gmail.map((g) => (
            <div key={g} className="flex items-center gap-2">
              <Mail className="size-4 text-muted" />
              <span className="flex-1 truncate text-[13px] text-fg">{g}</span>
              <Button size="sm" icon={Trash2} onClick={() => void save(native.gmailRemove(g))}>Remove</Button>
            </div>
          ))}
          <form
            className="flex flex-wrap gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void save(native.gmailAdd(address, password)).then(() => (setAddress(""), setPassword("")));
            }}
          >
            <Input placeholder="you@gmail.com" value={address} onChange={(e) => setAddress(e.target.value)} className="h-8 min-w-48 flex-1" />
            <Input placeholder="App password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} className="h-8 min-w-40 flex-1" />
            <Button type="submit" variant="outline" size="sm" icon={Plus} className="h-8" disabled={!address.includes("@") || !password}>Add</Button>
          </form>
        </div>
      </Group>
      <p className="mt-2 text-xs text-subtle">Turning autofill on or off, the Shortcut endpoint and Gmail apply after quitting BlackBubbles from the tray and starting it again. Everything else applies at once.</p>
    </Section>
  );
}
