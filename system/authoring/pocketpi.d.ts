type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Params = Json[] | { [key: string]: Json };
type FontSize = "sm" | "md" | "lg" | "xl";
type Color = "canvas" | "surface" | "shell" | "shellMuted" | "text" | "heading" |
  "muted" | "subtle" | "border" | "disabled" | "white" | "accent" | "accentSoft" |
  "info" | "infoSoft" | "success" | "successSoft" | "warning" | "warningText" |
  "warningSoft" | "danger" | "dangerSoft" | "dangerOnDark";

interface State<T> {
  get(): T;
  set(next: T | ((current: T) => T)): T;
  update(patch: Partial<T>): T;
}

interface Style {
  width?: number | "full"; height?: number | "full";
  minWidth?: number | "full"; minHeight?: number | "full";
  maxWidth?: number | "full"; maxHeight?: number | "full";
  padding?: number; paddingX?: number; paddingY?: number;
  paddingTop?: number; paddingRight?: number; paddingBottom?: number; paddingLeft?: number;
  margin?: number; marginX?: number; marginY?: number;
  marginTop?: number; marginRight?: number; marginBottom?: number; marginLeft?: number;
  gap?: number; direction?: "row" | "column";
  justify?: "start" | "center" | "end" | "between" | "around";
  align?: "start" | "center" | "end" | "stretch";
  grow?: number; shrink?: number; basis?: number; wrap?: boolean;
  position?: "relative" | "absolute"; top?: number; right?: number; bottom?: number; left?: number;
  display?: "flex" | "none"; overflow?: "visible" | "hidden"; zIndex?: number; hitPass?: boolean;
  background?: Color | number; borderColor?: Color | number; borderWidth?: number;
  radius?: number; opacity?: number; shadow?: number; color?: Color | number;
  fontSize?: FontSize; fontWeight?: "regular" | "bold";
  textAlign?: "left" | "center" | "right"; lineHeight?: number; tracking?: number;
  translateX?: number; translateY?: number; scale?: number; rotate?: number;
  scaleX?: number; scaleY?: number; originX?: number; originY?: number;
}

type Child = ViewRecipe | string | number | null | false | Child[];
interface ViewRecipe { readonly __viewRecipe?: true }
interface BaseProps { style?: Style; children?: Child }

declare const PocketPi: {
  readonly frameworkApi: 1;
  defineActions(actions: Record<string, (args: any, context: { source: "tool" | "ui" | "schedule" }) => any>): void;
  action(action: string, args?: Json): { type: "action"; action: string; args: Json };
  command(command: string, args?: Json): { type: "command"; command: string; args: Json };
  navigate(app: string): { type: "command"; command: "apps.open"; args: { app: string } };
  data: {
    query<T = Record<string, Json>>(sql: string, params?: Params): T[];
    exec(sql: string): void;
    transaction<T>(action: () => T): T;
    commit(): void;
  };
  resources: { get<T extends Json = Json>(name: string): T };
  services: { call<T extends Json = Json>(service: string, operation: string, args?: Json): T };
  actionContext: { remainingMs(): number };
  projection: {
    one<T>(sql: string, params: Params | (() => Params), apply: (row: T | null) => void): { refresh(): void };
    many<T>(sql: string, params: Params | (() => Params), apply: (rows: T[]) => void): { refresh(): void };
  };
};

declare const View: {
  readonly viewport: { width: number; height: number; orientation: "landscape" | "portrait"; scale: number; layoutWidth: number; layoutHeight: number };
  readonly colors: Record<Color, number>;
  state<T>(initial: T): State<T>;
  measureText(text: string, style?: Pick<Style, "fontSize" | "fontWeight">): number;
  mount(render: () => ViewRecipe, onDataChanged?: () => void): void;
  Box(props?: BaseProps): ViewRecipe;
  Row(props?: BaseProps): ViewRecipe;
  Column(props?: BaseProps): ViewRecipe;
  Text(props?: { text?: string | number | (() => string | number); style?: Style; children?: string | number }): ViewRecipe;
  Pressable(props: BaseProps & { onPress: () => any }): ViewRecipe;
  Screen(props?: BaseProps): ViewRecipe;
  Card(props?: BaseProps): ViewRecipe;
  Header(props: { title: string; metaTop?: string; metaBottom?: string; accent?: "busy" | "danger" | "none"; onBack?: () => any }): ViewRecipe;
  PageIntro(props: { eyebrow: string; title: string; description: string; tone?: "info" }): ViewRecipe;
  SectionHeading(props: { title: string; detail?: string; action?: boolean }): ViewRecipe;
  ActionButton(props: { label: string; onPress?: () => any; disabled?: boolean; tone?: "neutral" | "danger"; style?: Style }): ViewRecipe;
  Checkbox(props: { label: string; checked: boolean; onChange?: (checked: boolean) => any; disabled?: boolean; style?: Style }): ViewRecipe;
  Badge(props: { label: string; tone?: "neutral" | "info" | "success" | "warning" | "danger" }): ViewRecipe;
  EmptyState(props: { title: string; detail?: string; icon?: string; tone?: "info"; compact?: boolean; style?: Style }): ViewRecipe;
  MetricCard(props: { label: string; value: string; tone?: "success" | "danger" }): ViewRecipe;
  StatusBar(props: { text: string; tone?: "danger" | "neutral"; dark?: boolean }): ViewRecipe;
  NavigationBar(props: { items: Array<{ label: string; active?: boolean; onPress: () => any }> }): ViewRecipe;
  ScrollButton(props: { direction: "up" | "down"; onPress: () => any; style?: Style }): ViewRecipe;
  ScrollRail(props: { onUp: () => any; onDown: () => any; style?: Style }): ViewRecipe;
  Keyboard(props: { layer: "lower" | "upper" | "symbols"; onKey: (key: string) => any }): ViewRecipe;
  Sparkline(props: { values: Array<number | null>; labels?: string[]; tone?: Color; empty?: string }): ViewRecipe;
};

interface PocketPiResponse {
  readonly status: number; readonly url: string; readonly headers: Record<string, string>; readonly ok: boolean;
  bytes(): Promise<Uint8Array>; arrayBuffer(): Promise<ArrayBuffer>; text(): Promise<string>; json<T = Json>(): Promise<T>;
}
declare function fetch(url: string, options?: {
  method?: string; headers?: Record<string, string>; body?: string | Uint8Array | ArrayBuffer;
  timeoutMs?: number; maxBytes?: number;
}): Promise<PocketPiResponse>;
