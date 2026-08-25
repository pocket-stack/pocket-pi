import type { CSSProperties } from "react";
import { AppRevisionStory } from "./app-revision-story";
import { PocketPiDeviceStage } from "./pocketpi-device-stage";
import { githubUrl, SiteFooter, SiteHeader } from "./site-chrome";

export default function Home() {
  return (
    <div className="site-shell home-shell home-v6">
      <SiteHeader active="home" />
      <main>
        <section className="hero home-hero system-hero">
          <div className="container system-hero-grid">
            <div className="hero-copy system-hero-copy">
              <p className="eyebrow">PocketPi</p>
              <h1>The agent-native runtime <span>on embedded devices</span></h1>
              <p className="hero-lede">
                PocketPi runs the full Pi Agent core harness on-device, where it can operate,
                build and evolve the Apps that make up the product.
              </p>
              <div className="hero-actions">
                <a className="button button-primary" href="/docs/getting-started">Explore the runtime <span>→</span></a>
                <a className="button button-secondary" href={githubUrl} target="_blank" rel="noreferrer">GitHub <span>↗</span></a>
              </div>
            </div>
            <PocketPiDeviceStage />
          </div>
        </section>

        <section className="home-section actor-section" id="architecture" data-section="architecture">
          <div className="container">
            <div className="focused-heading actor-heading">
              <h2>The agent is not added to the product, it&apos;s part of the runtime.</h2>
              <p>Human and Pi Agent are independent actors. Both meet at the same App boundary.</p>
            </div>

            <figure className="actor-system-map" aria-label="The human acts through an App View while the resident Pi Agent acts through Tools inside the PocketPi runtime">
              <div className="human-actor">
                <div className="human-symbol" aria-hidden="true"><i /><span /></div>
                <strong>Human</strong>
                <small>outside the device runtime</small>
              </div>

              <div className="human-runtime-link" aria-hidden="true">
                <span>understands through View</span>
                <i />
                <span>acts through UI intent</span>
              </div>

              <div className="runtime-boundary">
                <div className="runtime-boundary-heading">
                  <strong>PocketPi runtime</strong>
                  <span>resident on the device</span>
                </div>

                <div className="runtime-actors">
                  <div className="runtime-app-node">
                    <div className="runtime-node-title"><strong>App</strong><span>shared product boundary</span></div>
                    <div className="runtime-app-parts">
                      <span><b>View</b><small>human projection</small></span>
                      <span><b>Actions</b><small>bounded operations</small></span>
                      <span><b>Data</b><small>durable truth</small></span>
                    </div>
                  </div>

                  <div className="agent-app-link" aria-hidden="true">
                    <span>inspect</span><i />
                    <span>invoke</span><i />
                    <span>revise</span><i />
                  </div>

                  <div className="resident-agent-node">
                    <span>resident system guest</span>
                    <strong>Pi Agent</strong>
                    <small>full core harness</small>
                    <div><b>Tools</b><b>Context</b><b>Model loop</b></div>
                  </div>
                </div>
              </div>
            </figure>
          </div>
        </section>

        <section className="home-section substrate-section-v6" id="pocketjs" data-section="substrate">
          <div className="container substrate-grid-v6">
            <div className="substrate-copy-v6">
              <h2>PocketJS is the runtime substrate.</h2>
              <p>
                PocketJS provides the one QuickJS execution platform, native rendering core and
                bounded host modules. PocketPi adds the resident Agent, App lifecycle and product model above it.
              </p>
              <p>
                The shared substrate keeps performance predictable as multiple Apps stay resident and multiple
                Actions run, without duplicating rendering or lifecycle control.
              </p>
              <a className="text-link" href="/docs/layers-ownership">Read the layer boundaries →</a>
            </div>

            <figure className="resource-footprint" aria-labelledby="resource-chart-title">
              <figcaption id="resource-chart-title">
                <div>
                  <span>Validated ESP32-S3 release footprint</span>
                  <small>Waveshare ESP32-S3-WROOM-1-N16R8</small>
                </div>
                <strong>Less than 35% used in every measured capacity</strong>
              </figcaption>

              <div className="resource-rings">
                <article className="resource-ring-item">
                  <div className="resource-ring" style={{ "--usage": "18.3%" } as CSSProperties}>
                    <div><strong>18.3%</strong><span>used</span></div>
                  </div>
                  <h3>PSRAM</h3>
                  <p><strong>1.46 MiB</strong> fixed scanout of 8 MiB</p>
                  <small>81.7% remains</small>
                </article>

                <article className="resource-ring-item">
                  <div className="resource-ring" style={{ "--usage": "30%" } as CSSProperties}>
                    <div><strong>30.0%</strong><span>used</span></div>
                  </div>
                  <h3>IRAM</h3>
                  <p><strong>100.0 KiB</strong> image of 333.75 KiB</p>
                  <small>70.0% remains</small>
                </article>

                <article className="resource-ring-item">
                  <div className="resource-ring" style={{ "--usage": "34.9%" } as CSSProperties}>
                    <div><strong>34.9%</strong><span>used</span></div>
                  </div>
                  <h3>Flash</h3>
                  <p><strong>5.59 MiB</strong> executable image of 16 MiB</p>
                  <small>65.1% remains</small>
                </article>
              </div>

              <div className="footprint-capabilities">
                <span>Inside this footprint</span>
                <p>Pi Agent core harness, PocketJS, App runtime, View SDK, SQLite, networking and touch UI.</p>
              </div>
              <p className="footprint-method">Flash and IRAM come from the current release ELF. PSRAM shows the fixed two-framebuffer floor. Dynamic Agent heaps and worker stacks are excluded.</p>
            </figure>
          </div>
        </section>

        <section className="home-section app-model-section" id="apps" data-section="app-definition">
          <div className="container">
            <div className="focused-heading app-model-heading">
              <h2>Apps are the unit the agent and human can both understand and act on.</h2>
              <p>Different actor surfaces converge on the same Actions and the same durable product truth.</p>
            </div>

            <div className="app-model-layout">
              <figure className="app-architecture-figure">
                {/* This local SVG is served directly; image optimization adds no value. */}
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src="/pocketpi-app-architecture.svg"
                  alt="Human View and Pi Agent Tools invoke the same App Actions over one App-owned SQLite database"
                />
              </figure>
              <div className="app-model-key">
                <div className="app-composition" aria-label="App equals Data plus Actions plus View">
                  <div className="app-composition-name"><span>App</span><i>=</i></div>
                  <div className="app-composition-parts">
                    <div><strong>Data</strong><p>App-owned SQLite and files.</p></div>
                    <i>+</i>
                    <div><strong>Actions</strong><p>Operations shared by UI and Agent.</p></div>
                    <i>+</i>
                    <div><strong>View</strong><p>The human-facing projection.</p></div>
                  </div>
                </div>
                <a className="text-link" href="/docs/app-guide">Read the complete App contract →</a>
              </div>
            </div>
          </div>
        </section>

        <section className="home-section iteration-section-v6" id="agent-iteration" data-section="capability">
          <div className="container">
            <div className="iteration-intro-v6">
              <div className="focused-heading iteration-heading-v6">
                <h2>The Agent develops the product it lives on.</h2>
                <p>This is a vision of the full agent-native development environment on an embedded device.</p>
              </div>
              <aside className="iteration-environment-overview">
                <strong>Why this is a development environment</strong>
                <p>
                  Editable source, file Tools, runtime validation, staged review and durable App Data form the
                  smallest complete development loop: describe a change, build it on-device, inspect the candidate
                  and let a human activate it.
                </p>
              </aside>
            </div>

            <AppRevisionStory />
          </div>
        </section>

        <section className="home-section ready-devices-section" id="devices" data-section="devices">
          <div className="container">
            <div className="focused-heading ready-heading">
              <h2>PocketPi-ready devices</h2>
              <p>The runtime and App contract have been implemented and physically validated on two resource-constrained Waveshare boards.</p>
            </div>

            <div className="ready-device-grid">
              <article className="ready-device">
                <div className="device-photo">
                  {/* The source photo is pre-cropped and stripped of metadata. */}
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img
                    src="/device-photos/waveshare-esp32-p4-wifi6-touch-lcd-5-back-black.png"
                    alt="Back of the Waveshare ESP32-P4-WIFI6-Touch-LCD-5 board showing the P4 module, connectors and PCB traces"
                    width="1448"
                    height="1086"
                    loading="lazy"
                    decoding="async"
                  />
                </div>
                <div className="ready-device-info">
                  <h3>Waveshare ESP32-P4-WIFI6-Touch-LCD-5</h3>
                  <p>ESP32-P4NRW32 / 32 MB PSRAM / 5-inch 720 × 1280 touch display</p>
                </div>
              </article>
              <article className="ready-device">
                <div className="device-photo">
                  {/* The source photo is pre-cropped and stripped of metadata. */}
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img
                    src="/device-photos/waveshare-esp32-s3-touch-lcd-4.3-back-black.png"
                    alt="Back of the Waveshare ESP32-S3-Touch-LCD-4.3 board showing the S3 module, ribbon connector and edge ports"
                    width="1448"
                    height="1086"
                    loading="lazy"
                    decoding="async"
                  />
                </div>
                <div className="ready-device-info">
                  <h3>Waveshare ESP32-S3-Touch-LCD-4.3</h3>
                  <p>ESP32-S3-WROOM-1-N16R8 / 8 MB PSRAM / 4.3-inch 800 × 480 touch display</p>
                </div>
              </article>
            </div>

            <div className="ready-devices-footer">
              <p>Device support is evidence for the runtime architecture. It is not the definition of PocketPi.</p>
              <a className="button button-primary" href="/docs/validation-status">Read validation status <span>→</span></a>
            </div>
          </div>
        </section>
      </main>
      <SiteFooter />
    </div>
  );
}
