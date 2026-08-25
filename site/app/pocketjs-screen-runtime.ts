export const POCKETPI_SCREEN_WIDTH = 480;
export const POCKETPI_SCREEN_HEIGHT = 800;

export type PocketPiDemoScreen = "Main" | "Apps" | "Files";
type SimulatorScreen = PocketPiDemoScreen | "Settings";

const screenAssets: Record<SimulatorScreen, string> = {
  Main: "/pocketpi-device/screens/main.png",
  Apps: "/pocketpi-device/screens/apps.png",
  Files: "/pocketpi-device/screens/files.png",
  Settings: "/pocketpi-device/screens/settings.png",
};

/**
 * Displays deterministic frames rendered by PocketPi's real ESP32 simulator
 * at the S3 host viewport (480 x 800 after physical panel rotation).
 *
 * These are not redesigned web mockups: the source frames come from
 * `pocket-pi-esp32-sim --viewport 480x800`. The 3D stage maps taps on the
 * model's screen back into these exact logical coordinates.
 */
export class PocketPiScreenRuntime {
  private images = new Map<SimulatorScreen, ImageBitmap>();

  private constructor(
    canvas: HTMLCanvasElement,
    private readonly context: CanvasRenderingContext2D,
    private readonly onBlit: () => void,
  ) {
    canvas.width = POCKETPI_SCREEN_WIDTH;
    canvas.height = POCKETPI_SCREEN_HEIGHT;
    context.imageSmoothingEnabled = false;
  }

  static async mount(canvas: HTMLCanvasElement, onBlit: () => void, signal: AbortSignal) {
    const context = canvas.getContext("2d", { alpha: false });
    if (!context) throw new Error("The PocketPi simulator canvas has no 2D context");
    const runtime = new PocketPiScreenRuntime(canvas, context, onBlit);
    await runtime.boot(signal);
    return runtime;
  }

  private async boot(signal: AbortSignal) {
    const loaded = await Promise.all(Object.entries(screenAssets).map(async ([screen, url]) => {
      const response = await fetch(url, { signal });
      if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
      return [screen as SimulatorScreen, await createImageBitmap(await response.blob())] as const;
    }));
    if (signal.aborted) {
      loaded.forEach(([, image]) => image.close());
      throw new DOMException("PocketPi simulator frame loading was cancelled", "AbortError");
    }
    this.images = new Map(loaded);
    this.show("Main");
  }

  show(screen: PocketPiDemoScreen) {
    this.draw(screen);
  }

  tap(x: number, y: number) {
    if (y < 735) return;
    const screen: SimulatorScreen = x < 120
      ? "Main"
      : x < 240
        ? "Files"
        : x < 360
          ? "Apps"
          : "Settings";
    this.draw(screen);
  }

  private draw(screen: SimulatorScreen) {
    const image = this.images.get(screen);
    if (!image) return;
    this.context.clearRect(0, 0, POCKETPI_SCREEN_WIDTH, POCKETPI_SCREEN_HEIGHT);
    this.context.drawImage(image, 0, 0, POCKETPI_SCREEN_WIDTH, POCKETPI_SCREEN_HEIGHT);
    this.onBlit();
  }

  destroy() {
    this.images.forEach((image) => image.close());
    this.images.clear();
  }
}
