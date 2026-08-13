import { Text, View } from "@pocketjs/framework/components";

// Pi Design v0.2. These are product-wide visual contracts built only from
// PocketJS public primitives. App data, navigation and side effects stay with
// the consuming App.
const type = {
  appTitle: "text-2xl text-white font-bold",
  pageTitle: "text-2xl text-slate-950 font-bold",
  heading: "text-xl text-slate-900 font-bold",
  label: "text-base text-slate-600 font-bold",
  captionStrong: "text-base text-slate-500 font-bold",
} as const;

export const statusBadge = {
  neutral: { surface: "px-3 py-2 rounded-lg bg-slate-100", text: "text-base text-slate-600 font-bold" },
  info: { surface: "px-3 py-2 rounded-lg bg-indigo-100", text: "text-base text-indigo-700 font-bold" },
  success: { surface: "px-3 py-2 rounded-lg bg-emerald-100", text: "text-base text-emerald-700 font-bold" },
  warning: { surface: "px-3 py-2 rounded-lg bg-amber-100", text: "text-base text-amber-700 font-bold" },
  danger: { surface: "px-3 py-2 rounded-lg bg-red-100", text: "text-base text-red-500 font-bold" },
} as const;

type HeaderProps = {
  title: string;
  back?: boolean;
  accent?: "ready" | "busy" | "danger" | "none";
  metaTop?: string;
  metaBottom?: string;
};

export function PocketHeader(props: HeaderProps) {
  const accent = () => props.accent === "busy" ? "w-[34] h-[34] rounded-lg bg-amber-400"
    : props.accent === "danger" ? "w-[34] h-[34] rounded-lg bg-red-500"
    : props.accent === "none" ? "w-[34] h-[34] rounded-lg bg-slate-800"
    : "w-[34] h-[34] rounded-lg bg-emerald-500";
  return (
    <View class="h-[112] px-6 flex-row items-center justify-between bg-slate-950">
      <View class="flex-row items-center gap-4">
        {props.back
          ? <Text class="w-[34] text-2xl text-white font-bold">‹</Text>
          : <View class={accent()} />}
        <Text class={type.appTitle}>{props.title}</Text>
      </View>
      <View class="w-[332] flex-col items-end gap-2">
        <Text class="text-base text-slate-300 font-bold">{props.metaTop ?? ""}</Text>
        <Text class="text-base text-slate-400">{props.metaBottom ?? ""}</Text>
      </View>
    </View>
  );
}

export function PageIntro(props: { eyebrow: string; title: string; description: string; tone?: "brand" | "info" }) {
  return (
    <View class="h-[166] px-6 pt-6 flex-col gap-3">
      <Text class={props.tone === "info" ? "text-base text-indigo-700 font-bold" : "text-base text-orange-600 font-bold"}>{props.eyebrow}</Text>
      <Text class={type.pageTitle}>{props.title}</Text>
      <Text class="text-lg text-slate-600">{props.description}</Text>
    </View>
  );
}

export function SectionHeading(props: { title: string; detail?: string; action?: boolean }) {
  const trailing = () => props.action
    ? (props.detail ? props.detail + "  ·  VIEW ALL  ›" : "VIEW ALL  ›")
    : props.detail ?? "";
  return (
    <View class="h-[44] px-1 flex-row items-center justify-between">
      <Text class={type.heading}>{props.title}</Text>
      <Text class={type.captionStrong}>{trailing()}</Text>
    </View>
  );
}

export function ActionButton(props: { label: string; disabled?: boolean; tone?: "primary" | "danger" | "neutral" }) {
  const container = () => props.disabled ? "w-full h-full items-center justify-center rounded-xl bg-slate-200"
    : props.tone === "danger" ? "w-full h-full items-center justify-center rounded-xl bg-red-100"
    : props.tone === "neutral" ? "w-full h-full items-center justify-center rounded-xl bg-slate-100"
    : "w-full h-full items-center justify-center rounded-xl bg-orange-600";
  const label = () => props.disabled ? "text-lg text-slate-500 font-bold"
    : props.tone === "danger" ? "text-lg text-red-500 font-bold"
    : props.tone === "neutral" ? "text-lg text-slate-900 font-bold"
    : "text-lg text-white font-bold";
  return <View class={container()}><Text class={label()}>{props.label}</Text></View>;
}

export function EmptyState(props: { icon?: string; title: string; detail?: string; tone?: "info" | "neutral"; compact?: boolean }) {
  return (
    <View class={props.compact ? "w-full h-[150] px-5 flex-col items-center justify-center rounded-xl shadow bg-white border-slate-100" : "w-full h-[430] px-12 flex-col items-center justify-center rounded-xl shadow bg-white border-slate-100"}>
      {props.icon ? <View class={props.tone === "info" ? "w-[88] h-[88] items-center justify-center rounded-xl bg-indigo-100" : "w-[88] h-[88] items-center justify-center rounded-xl bg-slate-100"}><Text class={props.tone === "info" ? "text-2xl text-indigo-700 font-bold" : "text-2xl text-slate-600 font-bold"}>{props.icon}</Text></View> : null}
      <Text class={props.icon ? "pt-7 text-2xl text-slate-900 font-bold" : "text-lg text-slate-500 font-bold"}>{props.title}</Text>
      {props.detail ? <Text class="pt-4 text-lg text-slate-500">{props.detail}</Text> : null}
    </View>
  );
}

export function MetricCard(props: { label: string; value: string; tone?: "neutral" | "success" | "danger" }) {
  const valueClass = () => props.tone === "success" ? "text-xl text-emerald-600 font-bold"
    : props.tone === "danger" ? "text-xl text-red-500 font-bold"
    : "text-xl text-slate-900 font-bold";
  return (
    <View class="w-full h-full px-5 py-4 flex-col gap-3 rounded-xl shadow bg-white border-slate-100">
      <Text class={type.label}>{props.label}</Text>
      <Text class={valueClass()}>{props.value}</Text>
    </View>
  );
}

export function StatusBar(props: { text: string; tone?: "neutral" | "danger"; dark?: boolean }) {
  const textClass = () => props.dark
    ? (props.tone === "danger" ? "text-base text-red-300" : "text-base text-slate-300")
    : (props.tone === "danger" ? "text-base text-red-500" : "text-base text-slate-500");
  return <View class={props.dark ? "w-full h-full px-6 flex-row items-center bg-slate-950" : "w-full h-full flex-row items-center"}><Text class={textClass()}>{props.text}</Text></View>;
}

export function ScrollButtons(props: { top: string; bottom: string }) {
  return (
    <>
      <View class={props.top}><Text class="text-base text-orange-600 font-bold">UP</Text></View>
      <View class={props.bottom}><Text class="text-base text-orange-600 font-bold">DN</Text></View>
    </>
  );
}
