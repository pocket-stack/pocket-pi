import { Text, View } from "@pocketjs/framework/components";

// Pi Agent's build-time Pi Design recipes. Ordinary source Apps use the
// equivalent runtime-owned components in system/view-sdk.js.
const type = {
  appTitle: "text-2xl text-white font-bold",
  pageTitle: "text-2xl text-slate-950 font-bold",
  heading: "text-xl text-slate-900 font-bold",
  captionStrong: "text-base text-slate-500 font-bold",
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
