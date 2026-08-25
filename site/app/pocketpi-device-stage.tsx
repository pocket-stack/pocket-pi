"use client";

import { useEffect, useRef, useState } from "react";
import type { BufferGeometry, CanvasTexture, Material, Mesh, Object3D, PerspectiveCamera, Scene, WebGLRenderer } from "three";
import type { PocketPiDemoScreen, PocketPiScreenRuntime } from "./pocketjs-screen-runtime";

const demoScreens: PocketPiDemoScreen[] = ["Main", "Apps", "Files"];
const S3_SCREEN_WIDTH = 480;
const S3_SCREEN_HEIGHT = 800;
type HeroVisual = "Device" | "Architecture";
type ThreeModule = typeof import("three");

function addPortraitScreenUv(THREE: ThreeModule, geometry: BufferGeometry) {
  const position = geometry.getAttribute("position");
  geometry.computeBoundingBox();
  const bounds = geometry.boundingBox;
  if (!bounds) throw new Error("The screen primitive has no bounds");
  const width = Math.max(0.0001, bounds.max.z - bounds.min.z);
  const height = Math.max(0.0001, bounds.max.x - bounds.min.x);
  const uv = new Float32Array(position.count * 2);
  for (let index = 0; index < position.count; index++) {
    // The landscape board is presented vertically. Its original Z axis becomes
    // screen-right and its original X axis becomes screen-up.
    uv[index * 2] = (position.getZ(index) - bounds.min.z) / width;
    uv[index * 2 + 1] = (position.getX(index) - bounds.min.x) / height;
  }
  geometry.setAttribute("uv", new THREE.BufferAttribute(uv, 2));
}

function bindLiveScreen(THREE: ThreeModule, model: Object3D, texture: CanvasTexture) {
  const screenMeshes: Mesh[] = [];
  model.traverse((object) => {
    if (!(object instanceof THREE.Mesh)) return;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    const nextMaterials = materials.map((material) => {
      if (material.name !== "screen_off") return material;
      screenMeshes.push(object);
      object.geometry = object.geometry.clone();
      addPortraitScreenUv(THREE, object.geometry);
      return new THREE.MeshBasicMaterial({
        name: "pocketpi_live_screen",
        map: texture,
        color: 0xffffff,
        side: THREE.DoubleSide,
        toneMapped: false,
      });
    });
    object.material = Array.isArray(object.material) ? nextMaterials : nextMaterials[0];
  });
  if (screenMeshes.length !== 1) {
    throw new Error(`Expected one screen material, found ${screenMeshes.length}`);
  }
  return screenMeshes[0];
}

export function PocketPiDeviceStage() {
  const rootRef = useRef<HTMLElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const screenCanvasRef = useRef<HTMLCanvasElement>(null);
  const statusRef = useRef<HTMLSpanElement>(null);
  const [activeVisual, setActiveVisual] = useState<HeroVisual>("Device");

  useEffect(() => {
    const root = rootRef.current;
    const viewport = viewportRef.current;
    const canvas = canvasRef.current;
    const screenCanvas = screenCanvasRef.current;
    if (!root || !viewport || !canvas || !screenCanvas) return;

    const abortController = new AbortController();
    let disposed = false;
    let inViewport = true;
    let renderFrame = 0;
    let screenTimer = 0;
    let userTouchedScreen = false;
    let renderer: WebGLRenderer | null = null;
    let screenRuntime: PocketPiScreenRuntime | null = null;
    let modelRoot: Object3D | null = null;
    let screenTexture: CanvasTexture | null = null;
    let scene: Scene | null = null;
    let camera: PerspectiveCamera | null = null;
    let threeModule: ThreeModule | null = null;

    const render = () => {
      renderFrame = 0;
      if (!renderer || !scene || !camera || !inViewport || document.hidden) return;
      renderer.render(scene, camera);
    };
    const invalidate = () => {
      if (!renderer || renderFrame || !inViewport || document.hidden) return;
      renderFrame = requestAnimationFrame(render);
    };

    const resize = () => {
      if (!renderer || !camera) return;
      const width = Math.max(1, viewport.clientWidth);
      const height = Math.max(1, viewport.clientHeight);
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      invalidate();
    };

    let resizeObserver: ResizeObserver | null = null;
    let visibilityObserver: IntersectionObserver | null = null;
    const onVisibilityChange = () => invalidate();

    const boot = async () => {
      try {
        const [THREE, loaderModule, runtimeModule] = await Promise.all([
          import("three"),
          import("three/addons/loaders/GLTFLoader.js"),
          import("./pocketjs-screen-runtime"),
        ]);
        if (disposed) return;
        threeModule = THREE;
        const { GLTFLoader } = loaderModule;
        const { PocketPiScreenRuntime } = runtimeModule;
        const stageScene = new THREE.Scene();
        const stageCamera = new THREE.PerspectiveCamera(33, 1, 0.05, 100);
        // A restrained three-quarter product angle exposes the PCB thickness
        // and connector relief while keeping the 480 x 800 UI legible.
        stageCamera.position.set(1.72, 0.46, 6.55);
        // Both lights stay in world space while the board itself rotates. The
        // highlights and shadows therefore change in real time during a drag.
        stageScene.add(new THREE.AmbientLight(0xffffff, 0.08));
        const key = new THREE.SpotLight(0xffffff, 62, 14, Math.PI * .0625, .28, 1.42);
        key.position.set(-4.8, 5.9, 7.6);
        key.target.position.set(-.18, .05, 0);
        key.castShadow = true;
        key.shadow.mapSize.set(2048, 2048);
        key.shadow.bias = -.0002;
        key.shadow.normalBias = .018;
        stageScene.add(key, key.target);
        const backFill = new THREE.SpotLight(0xf2f6ff, 22, 12, Math.PI * .15, .72, 1.35);
        backFill.position.set(3.8, 1.8, -5.4);
        backFill.target.position.set(.1, 0, 0);
        stageScene.add(backFill, backFill.target);
        scene = stageScene;
        camera = stageCamera;

        renderer = new THREE.WebGLRenderer({
          canvas,
          alpha: true,
          antialias: true,
          powerPreference: "high-performance",
          premultipliedAlpha: true,
        });
        renderer.setClearColor(0x000000, 0);
        renderer.setPixelRatio(Math.min(Math.max(window.devicePixelRatio || 1, 2), 2.5));
        renderer.outputColorSpace = THREE.SRGBColorSpace;
        renderer.toneMapping = THREE.NoToneMapping;
        renderer.toneMappingExposure = 1;
        renderer.shadowMap.enabled = true;
        renderer.shadowMap.type = THREE.PCFShadowMap;

        stageCamera.lookAt(0, 0, 0);
        canvas.style.touchAction = "pan-y";

        resizeObserver = new ResizeObserver(resize);
        resizeObserver.observe(viewport);
        visibilityObserver = new IntersectionObserver(([entry]) => {
          inViewport = entry.isIntersecting;
          if (inViewport) invalidate();
          else if (renderFrame) {
            cancelAnimationFrame(renderFrame);
            renderFrame = 0;
          }
        }, { threshold: 0.04 });
        visibilityObserver.observe(root);
        document.addEventListener("visibilitychange", onVisibilityChange);
        resize();

        screenTexture = new THREE.CanvasTexture(screenCanvas);
        screenTexture.colorSpace = THREE.SRGBColorSpace;
        // The simulator already renders at 2x its logical resolution. Avoid
        // mipmap minification here because it softens the small native text.
        screenTexture.generateMipmaps = false;
        screenTexture.minFilter = THREE.LinearFilter;
        screenTexture.magFilter = THREE.LinearFilter;

        const loader = new GLTFLoader();
        const [gltf, runtime] = await Promise.all([
          loader.loadAsync("/pocketpi-device/device.glb"),
          PocketPiScreenRuntime.mount(screenCanvas, () => {
            if (!screenTexture) return;
            screenTexture.needsUpdate = true;
            invalidate();
          }, abortController.signal),
        ]);
        if (disposed) {
          runtime.destroy();
          return;
        }
        screenRuntime = runtime;
        const screenMesh = bindLiveScreen(THREE, gltf.scene, screenTexture);
        gltf.scene.traverse((object) => {
          if (!(object instanceof THREE.Mesh) || object === screenMesh) return;
          object.castShadow = true;
          object.receiveShadow = true;
        });

        // Source model: X = width, Y = front depth, Z = height. First make
        // front face +Z, then rotate the landscape panel into the portrait UI
        // orientation used by PocketPi's 480 x 800 S3 View.
        const frontFacing = new THREE.Group();
        frontFacing.rotation.x = Math.PI / 2;
        frontFacing.add(gltf.scene);
        const portrait = new THREE.Group();
        portrait.rotation.z = Math.PI / 2;
        portrait.add(frontFacing);
        portrait.updateMatrixWorld(true);

        const bounds = new THREE.Box3().setFromObject(portrait);
        const size = bounds.getSize(new THREE.Vector3());
        const center = bounds.getCenter(new THREE.Vector3());
        portrait.position.copy(center).multiplyScalar(-1);
        const normalized = new THREE.Group();
        // Keep a restrained overlap with the Architecture card without letting
        // the hardware dominate the whole right-hand side of the Hero.
        normalized.scale.setScalar(3.16 / size.y);
        normalized.position.y = -.08;
        normalized.add(portrait);
        modelRoot = normalized;
        stageScene.add(normalized);

        const raycaster = new THREE.Raycaster();
        const pointer = new THREE.Vector2();
        const pickScreen = (clientX: number, clientY: number) => {
          const canvasBounds = canvas.getBoundingClientRect();
          pointer.x = ((clientX - canvasBounds.left) / canvasBounds.width) * 2 - 1;
          pointer.y = -((clientY - canvasBounds.top) / canvasBounds.height) * 2 + 1;
          raycaster.setFromCamera(pointer, stageCamera);
          const hits = raycaster.intersectObject(normalized, true);
          const screenHit = hits.find((hit) => hit.object === screenMesh);
          if (!screenHit || !hits[0]) return null;
          // Allow a thin glass/highlight primitive in front of the live panel,
          // but reject the screen when the opaque PCB is genuinely in front.
          return screenHit.distance - hits[0].distance < .12 ? screenHit : null;
        };
        let pointerStart: { x: number; y: number } | null = null;
        let rotationStart = { x: 0, y: 0 };
        let pointerDragged = false;
        canvas.addEventListener("pointerdown", (event) => {
          pointerStart = { x: event.clientX, y: event.clientY };
          rotationStart = { x: normalized.rotation.x, y: normalized.rotation.y };
          pointerDragged = false;
          canvas.setPointerCapture(event.pointerId);
        }, { signal: abortController.signal });
        canvas.addEventListener("pointermove", (event) => {
          if (pointerStart && Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 6) {
            pointerDragged = true;
            const deltaX = event.clientX - pointerStart.x;
            const deltaY = event.clientY - pointerStart.y;
            normalized.rotation.y = rotationStart.y + deltaX * .008;
            normalized.rotation.x = THREE.MathUtils.clamp(rotationStart.x + deltaY * .005, -.52, .52);
            canvas.style.cursor = "grabbing";
            invalidate();
            return;
          }
          canvas.style.cursor = pickScreen(event.clientX, event.clientY) ? "pointer" : "grab";
        }, { signal: abortController.signal });
        canvas.addEventListener("pointerup", (event) => {
          if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
        }, { signal: abortController.signal });
        canvas.addEventListener("pointercancel", () => {
          pointerStart = null;
          pointerDragged = false;
        }, { signal: abortController.signal });
        canvas.addEventListener("click", (event) => {
          const hit = pointerDragged ? null : pickScreen(event.clientX, event.clientY);
          pointerStart = null;
          pointerDragged = false;
          if (!hit?.uv) return;
          event.preventDefault();
          userTouchedScreen = true;
          window.clearInterval(screenTimer);
          const x = Math.round(hit.uv.x * (S3_SCREEN_WIDTH - 1));
          const y = Math.round((1 - hit.uv.y) * (S3_SCREEN_HEIGHT - 1));
          if (statusRef.current) statusRef.current.textContent = `Live S3 simulator, last tap ${x}, ${y}`;
          runtime.tap(x, y);
        }, { signal: abortController.signal });

        root.dataset.ready = "true";
        if (statusRef.current) statusRef.current.textContent = "S3 simulator frames ready";
        runtime.show("Main");
        screenTexture.needsUpdate = true;
        invalidate();

        const motionPreference = window.matchMedia("(prefers-reduced-motion: reduce)");
        let cycleIndex = 0;
        const syncScreenCycle = () => {
          window.clearInterval(screenTimer);
          if (motionPreference.matches || userTouchedScreen) return;
          screenTimer = window.setInterval(() => {
            cycleIndex = (cycleIndex + 1) % demoScreens.length;
            const screen = demoScreens[cycleIndex];
            runtime.show(screen);
          }, 6200);
        };
        syncScreenCycle();
        motionPreference.addEventListener("change", syncScreenCycle, { signal: abortController.signal });
      } catch (error) {
        if (disposed || (error instanceof DOMException && error.name === "AbortError")) return;
        root.dataset.error = "true";
        if (statusRef.current) statusRef.current.textContent = "Interactive preview unavailable";
        console.error("PocketPi device stage failed", error);
      }
    };

    void boot();

    return () => {
      disposed = true;
      abortController.abort();
      window.clearInterval(screenTimer);
      if (renderFrame) cancelAnimationFrame(renderFrame);
      resizeObserver?.disconnect();
      visibilityObserver?.disconnect();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      screenRuntime?.destroy();
      const geometries = new Set<BufferGeometry>();
      const materials = new Set<Material>();
      modelRoot?.traverse((object) => {
        if (!threeModule || !(object instanceof threeModule.Mesh)) return;
        const mesh = object as Mesh;
        geometries.add(mesh.geometry);
        const objectMaterials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
        objectMaterials.forEach((material) => materials.add(material));
      });
      geometries.forEach((geometry) => geometry.dispose());
      materials.forEach((material) => material.dispose());
      screenTexture?.dispose();
      renderer?.dispose();
    };
  }, []);

  return (
    <figure
      className="system-architecture-figure pocketpi-device-stage"
      data-view={activeVisual.toLowerCase()}
      aria-label={activeVisual === "Device"
        ? "Interactive PocketPi device displaying real S3 simulator frames"
        : "PocketPi runtime architecture"}
      ref={rootRef}
    >
      <div className="device-stage-architecture" aria-hidden="true">
        {/* This local SVG is deliberately served without a raster optimization hop. */}
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          className="device-stage-architecture-image"
          src="/pocketpi-system-architecture.svg"
          alt=""
        />
      </div>
      <span className="device-stage-status" ref={statusRef} aria-live="polite">
        Loading S3 simulator frames
      </span>
      <div className="device-stage-viewport" ref={viewportRef}>
        <canvas
          ref={canvasRef}
          className="device-stage-canvas"
          aria-label="Interactive 3D PocketPi device. Drag the hardware to rotate it, or tap controls rendered on its live simulator screen."
        />
      </div>
      <canvas ref={screenCanvasRef} className="device-stage-screen-canvas" aria-hidden="true" />
      <div className="device-stage-visual-switch" aria-label="Hero visual">
        {(["Device", "Architecture"] as HeroVisual[]).map((visual) => (
          <button
            type="button"
            key={visual}
            aria-pressed={activeVisual === visual}
            onClick={() => setActiveVisual(visual)}
          >
            {visual}
          </button>
        ))}
      </div>
    </figure>
  );
}
