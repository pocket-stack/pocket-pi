import { Text, View } from "@pocketjs/framework/components";

// Pocket Pi Design System v0.1. Keep this layer deliberately small: shared
// typography, spacing and foundational surfaces only. Product-specific
// components such as charts, account pickers and file rows stay in each App.
export const type = {
  title: "text-2xl text-white font-bold",
  section: "text-sm text-slate-600 font-bold",
  body: "text-base text-slate-900",
  bodyStrong: "text-base text-slate-900 font-bold",
  caption: "text-sm text-slate-500",
} as const;

export const space = {
  screenX: "px-6",
  card: "px-6 py-5",
  stack: "gap-4",
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
        <Text class={type.title}>{props.title}</Text>
      </View>
      <View class="w-[332] flex-col items-end gap-2">
        <Text class="text-sm text-slate-300 font-bold">{props.metaTop ?? ""}</Text>
        <Text class="text-sm text-slate-400">{props.metaBottom ?? ""}</Text>
      </View>
    </View>
  );
}

export function SectionHeading(props: { title: string; detail?: string; action?: boolean }) {
  const trailing = () => props.action
    ? (props.detail ? props.detail + "  ·  VIEW ALL  ›" : "VIEW ALL  ›")
    : props.detail ?? "";
  return (
    <View class="h-[40] px-1 flex-row items-center justify-between">
      <Text class="text-lg text-slate-900 font-bold">{props.title}</Text>
      <Text class="text-sm text-slate-500 font-bold">{trailing()}</Text>
    </View>
  );
}

export function ActionButton(props: { label: string; disabled?: boolean }) {
  return (
    <View class={props.disabled ? "w-full h-full items-center justify-center rounded-xl bg-slate-200" : "w-full h-full items-center justify-center rounded-xl bg-orange-600"}>
      <Text class={props.disabled ? "text-base text-slate-500 font-bold" : "text-base text-white font-bold"}>{props.label}</Text>
    </View>
  );
}
