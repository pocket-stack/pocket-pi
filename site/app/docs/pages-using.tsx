import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink } from "./doc-components";

export const USING_DOCS: DocRecord[] = [
  {
    slug: "simulator",
    title: "Run the simulator",
    description: "Use the macOS ESP32 product-contract simulator for Agent, App, viewport and install development.",
    render: () => <>
      <h1>Run the simulator</h1>
      <DocLead>
        The macOS simulator is the normal development host. It runs the same AgentOS supervisor,
        resident Agent guest, ordinary App source, Tool catalog, workspace contracts and PocketJS
        Views as both ESP32 hardware compositions while replacing board adapters with macOS implementations.
      </DocLead>
      <PageGoal>
        A repeatable development command, the right model backend, persistent state, App install and
        deterministic screenshot workflows, plus a precise understanding of what the simulator cannot prove.
      </PageGoal>

      <h2>Start with a persistent workspace</h2>
      <Code>{`cargo xtask run esp32-sim \\
  --backend codex \\
  --workspace target/esp32-workspace`}</Code>
      <p>
        The default workspace is <code>target/esp32-sim/workspace</code>. Supplying an explicit
        path makes test state obvious and lets separate App experiments use separate workspaces.
        Do not delete the directory if you want installed Apps and SQLite state to survive.
      </p>

      <h2>Choose a model backend</h2>
      <table>
        <thead><tr><th>Backend</th><th>Command</th><th>Credential source</th></tr></thead>
        <tbody>
          <tr><td>Codex</td><td><code>--backend codex</code></td><td>Existing local Codex Coding Plan login</td></tr>
          <tr><td>OpenAI</td><td><code>--backend openai --model gpt-5.6</code></td><td><code>OPENAI_API_KEY</code></td></tr>
          <tr><td>OpenRouter</td><td><code>--backend openrouter --model openai/gpt-5.6</code></td><td><code>OPENROUTER_API_KEY</code></td></tr>
          <tr><td>Anthropic</td><td><code>--backend anthropic --model claude-sonnet-4-6</code></td><td><code>ANTHROPIC_API_KEY</code></td></tr>
          <tr><td>DeepSeek</td><td><code>--backend deepseek</code></td><td><code>DEEPSEEK_API_KEY</code></td></tr>
        </tbody>
      </table>
      <Code>{`OPENAI_API_KEY=... \\
  cargo xtask run esp32-sim --backend openai --model gpt-5.6

OPENROUTER_API_KEY=... \\
  cargo xtask run esp32-sim \\
  --backend openrouter --model openai/gpt-5.6

ANTHROPIC_API_KEY=... \\
  cargo xtask run esp32-sim \\
  --backend anthropic --model claude-sonnet-4-6

DEEPSEEK_API_KEY=... DEEPSEEK_THINKING_LEVEL=xhigh \\
  cargo xtask run esp32-sim --backend deepseek`}</Code>
      <p>
        Backend selection belongs to the host composition. It does not change the resident App,
        ordinary App contract or Tool definitions.
      </p>

      <h2>Open a specific surface</h2>
      <Code>{`cargo xtask run esp32-sim --backend codex --app files
cargo xtask run esp32-sim --backend codex --app apps
cargo xtask run esp32-sim --backend codex --app settings
cargo xtask run esp32-sim --backend codex --app keyboard`}</Code>
      <p>
        These names start in the corresponding Pi Agent surface. An installed ordinary App id may
        also be supplied with <code>--app</code>. The default viewport is 720×1280. Use
        <code> --viewport 800x480</code> or <code>--viewport 480x800</code> to exercise the same App
        source at the S3 panel shape and rotated logical shape. Mouse input follows the same View
        hit-testing path as touch.
      </p>

      <h2>Install an App into the running simulator</h2>
      <Code>{`cargo xtask package app counter

curl --fail-with-body \\
  --data-binary @target/pocketapps/counter.pocketapp \\
  http://127.0.0.1:8080/install`}</Code>
      <p>
        The upload returns HTTP 202 and switches the product to the shared review screen. Click
        <strong> INSTALL</strong> inside the simulator to activate it. This is the same review and
        AppSupervisor lifecycle used by the physical HTTP ingress; the transport does not write App
        state directly.
      </p>

      <h2>Generate a deterministic screenshot</h2>
      <Code>{`cargo xtask snapshot esp32-sim`}</Code>
      <p>
        The output is <code>artifacts/screenshots/esp32-sim.png</code>. For targeted snapshots,
        invoke the simulator with <code>--screenshot</code>, <code>--app</code>, <code>--prompt</code>
        or <code>--tap x,y</code> through its lower-level Cargo command. The standard <code>xtask</code>
        command intentionally keeps the common path small.
      </p>

      <h2>Simulator fixtures</h2>
      <p>
        Exa and Robinhood use deterministic native service fixtures in the simulator. Their real App
        source, SQLite writes and Views run unchanged, but provider responses are simulated. Use a
        physical standalone backend for fresh provider and transport acceptance.
      </p>
      <Fact>
        Simulator success proves product contracts and macOS adapters. It does not prove ESP-IDF,
        PSRAM allocation, PPA, LittleFS capacity, NVS, LCD/touch, Wi-Fi or live provider behavior.
      </Fact>
    </>,
  },
  {
    slug: "pi-agent-workspace",
    title: "Pi Agent and workspace",
    description: "Use the resident Pi Agent, its durable workspace, native Tools and Agent wake schedules.",
    render: () => <>
      <h1>Pi Agent and <code>/workspace</code></h1>
      <DocLead>
        Pi Agent is the privileged resident System App. Its Agent loop, Tool registry and Root View
        share one long-lived Guest, and it alone owns the top-level <code>/workspace</code>.
      </DocLead>
      <PageGoal>
        A practical model of what the Agent can persist, which native Tools it receives, how Agent
        wakes differ from App schedules, and what currently resets on reboot.
      </PageGoal>

      <h2>Resident means one lifecycle</h2>
      <p>
        Firmware embeds the Pi Agent release so a blank device can boot. The System Guest is created
        once and remains outside the ordinary View/Action LRU caches. Opening an ordinary App changes
        only the foreground View. It does not reset the conversation, drop an active model request or
        rebuild the Tool registry.
      </p>

      <h2>Workspace is Agent-owned durable state</h2>
      <p>
        On ESP32-P4 and ESP32-S3, <code>/workspace</code> lives in LittleFS. In the simulator it maps to the directory
        supplied by <code>--workspace</code>. Agent files survive Guest activity and device restart.
        This is separate from every ordinary App&apos;s private data root.
      </p>
      <Code>{`/workspace/
├── memory.md                 Agent-managed files
├── notes.txt
├── .system/                  runtime-owned state
│   └── app-events/<id>.json  recent install/update outcomes
├── system/app/               firmware-seeded Pi Agent release
└── apps/<id>/
    ├── release/              installed App source
    ├── checkout/             Agent-editable candidate, when present
    ├── data/                 App-owned SQLite/files
    └── tmp/                  disposable App files`}</Code>
      <p>
        The Agent uses bounded file Tools; it does not receive raw access to another App&apos;s SQLite,
        native credential store or arbitrary host filesystem.
      </p>

      <h2>Core native Tools</h2>
      <table>
        <thead><tr><th>Area</th><th>Tools</th><th>Purpose</th></tr></thead>
        <tbody>
          <tr><td>Workspace</td><td><code>read</code>, <code>write</code>, <code>edit</code>, <code>find</code>, <code>grep</code>, <code>ls</code></td><td>Durable Agent-managed files</td></tr>
          <tr><td>Device</td><td><code>device.status</code>, <code>time.now</code></td><td>Bounded runtime facts</td></tr>
          <tr><td>Context</td><td><code>workspace.context</code></td><td>Assemble durable workspace memory</td></tr>
          <tr><td>Agent wakes</td><td><code>schedule.set</code>, <code>schedule.list</code>, <code>schedule.cancel</code>, <code>schedule.clear</code></td><td>Prompt the Agent now or later</td></tr>
          <tr><td>Utility</td><td><code>bash</code></td><td>Allowlisted workspace/device commands</td></tr>
        </tbody>
      </table>
      <p>
        Embedded <code>bash</code> is a command dispatcher, not POSIX. There are no processes, pipes,
        package manager or unrestricted host shell. Its name preserves the familiar Agent Tool shape;
        its implementation remains bounded for the device.
      </p>

      <h2>Agent wake versus App schedule</h2>
      <table>
        <thead><tr><th></th><th>Agent wake</th><th>App schedule</th></tr></thead>
        <tbody>
          <tr><td>Owner</td><td>Pi Agent workspace</td><td>Ordinary App manifest/runtime</td></tr>
          <tr><td>Runs</td><td>A new prompt through the Agent loop</td><td>One named deterministic Action</td></tr>
          <tr><td>Best for</td><td>Reasoning, follow-up and cross-App coordination</td><td>Refresh, cleanup and synchronization</td></tr>
          <tr><td>Needs model</td><td>Yes</td><td>No</td></tr>
        </tbody>
      </table>

      <h2>What survives reboot today</h2>
      <ul>
        <li>Agent workspace files and Agent wake schedule state survive.</li>
        <li>Installed Apps, App SQLite/files and App schedule cursors survive.</li>
        <li>Wi-Fi and model credentials survive in native stores on hardware.</li>
        <li>The in-heap conversation does <strong>not</strong> survive; session persistence is not implemented.</li>
      </ul>
    </>,
  },
  {
    slug: "manage-apps",
    title: "Install and manage Apps",
    description: "Package, review, install, update and uninstall ordinary Apps without changing Firmware.",
    render: () => <>
      <h1>Install and manage Apps</h1>
      <DocLead>
        Ordinary Apps are complete source releases in a <code>.pocketapp</code> container. HTTP and
        UART are only ingress paths: both stop at the same on-product review screen and hand the
        package to the same AppSupervisor lifecycle.
      </DocLead>
      <PageGoal>
        Safe commands for first install and update, the physical confirmation flow, what state is
        preserved, and what uninstall removes.
      </PageGoal>

      <h2>Package a first install</h2>
      <Code>{`# App without credentials
cargo xtask package app counter

# App whose manifest declares credentials
cargo xtask package app exa path/to/exa-credentials.json`}</Code>
      <p>
        Output is written to <code>target/pocketapps/&lt;id&gt;.pocketapp</code> with file mode 0600 on
        Unix. For a credentialed first install, <code>credentials.json</code> must contain exactly the
        credential ids declared by <code>app.json</code>, with no missing or extra keys.
      </p>

      <h2>Upload over the local network</h2>
      <p>
        Open <code>http://&lt;device-ip&gt;/</code> from a computer or phone and choose the package, or
        upload directly:
      </p>
      <Code>{`curl --fail-with-body \\
  --data-binary @target/pocketapps/exa.pocketapp \\
  http://DEVICE_IP/install`}</Code>

      <h2>Upload over USB UART</h2>
      <Code>{`python3 tools/uart-install.py "$DEVICE_PORT" \\
  target/pocketapps/exa.pocketapp`}</Code>
      <p>
        UART upload does not provision a model, overwrite Wi-Fi, reset the board or bypass review.
        It transfers the same complete package to the same Installer. Set <code>DEVICE_PORT</code>
        using the discovery step on the <a href="/docs/esp32-p4">P4</a> or
        <a href="/docs/esp32-s3"> S3</a> target page first.
      </p>

      <h2>Review on the product</h2>
      <p>
        The runtime validates the archive before showing review: identity, size, source files,
        Framework API, capabilities, native service policy, resources and credential declarations.
        A person then sees the App name, version, Tool count, schedules, network/credential needs and
        whether this is a fresh install or update. Activation starts only after confirmation.
      </p>

      <h2>Package an update</h2>
      <Code>{`# Update packages omit credentials
cargo xtask package app exa`}</Code>
      <p>
        An update keeps the same App id and native permissions. PocketPi preserves App SQLite
        data and already stored credentials, rehearses the candidate source and any migrations on a
        copied database, then swaps the single active source release. Updates that carry credentials,
        change native permissions, downgrade the schema or skip a migration are rejected.
      </p>

      <h2>Let Pi Agent iterate an installed App</h2>
      <ol>
        <li>Ask Pi Agent to call <code>app.checkout</code> with the installed ordinary App id.</li>
        <li>The Tool returns <code>apps/&lt;id&gt;/checkout</code> plus the latest <code>.system/app-events/&lt;id&gt;.json</code> outcome file.</li>
        <li>The Agent reads the previous outcome, edits only the checkout with normal file Tools and advances <code>app.json</code> <code>version</code>.</li>
        <li>For a SQLite shape change, it also advances <code>schemaVersion</code> and adds every required <code>migrations/N.sql</code> step.</li>
        <li>The Agent calls <code>app.submit</code> with the exact checkout path.</li>
        <li>PocketPi validates and stages the candidate, then opens the same review screen used by HTTP and UART. Nothing changes until a person confirms.</li>
      </ol>
      <Code>{`app.checkout({ "id": "exa" })
# edit apps/exa/checkout/app.json
# edit apps/exa/checkout/actions.js or view.js
app.submit({ "path": "apps/exa/checkout" })`}</Code>
      <p>
        Checkout copies source once and reopens existing Agent work on later calls. It does not copy
        <code>data/</code>, <code>tmp/</code> or credentials. Submit moves the candidate into the
        existing installer staging area instead of creating a parallel update mechanism.
      </p>

      <h2>Uninstall</h2>
      <p>
        Open <strong>Apps</strong>, choose <strong>UNINSTALL APP</strong>, then tap the App&apos;s
        <strong> X</strong>. Uninstall removes:
      </p>
      <ul>
        <li>the App source release and complete private data root;</li>
        <li>SQLite databases, files and schedule state;</li>
        <li>public Tool routes and cached View/Action Guests;</li>
        <li>native credentials and native MCP session state.</li>
      </ul>
      <Fact>
        Uninstall is destructive and there is no rollback. The resident Pi Agent System App cannot
        be installed, updated or uninstalled through the ordinary App lifecycle.
      </Fact>

      <h2>Common install failures</h2>
      <table>
        <thead><tr><th>Message or symptom</th><th>Meaning</th><th>Fix</th></tr></thead>
        <tbody>
          <tr><td><code>credentials.json ids do not match app.json</code></td><td>First-install secret keys differ from manifest bindings</td><td>Supply exactly the declared ids; omit credentials for an update</td></tr>
          <tr><td><code>unsupported Framework API</code></td><td>App targets a different System Framework contract</td><td>Set <code>frameworkApi</code> to the supported value or update runtime intentionally</td></tr>
          <tr><td><code>another install is pending</code></td><td>A review already owns the install slot</td><td>Confirm or dismiss it on the product</td></tr>
          <tr><td>Update rejected before mutation</td><td>Permission/schema/migration contract failed</td><td>Correct the candidate; installed source/data remain active</td></tr>
        </tbody>
      </table>
    </>,
  },
  {
    slug: "esp32-p4",
    title: "ESP32-P4 reference target",
    description: "Build, flash, provision and validate PocketPi on the first supported physical target.",
    render: () => <>
      <h1>ESP32-P4 reference target</h1>
      <DocLead>
        The Waveshare ESP32-P4-WIFI6-Touch-LCD-5 is the first fully supported hardware target and the
        current reference implementation. It is where PocketPi must prove standalone boot,
        local state, touch/display, Wi-Fi, provider transport and App lifecycle under real constraints.
      </DocLead>
      <PageGoal>
        The complete build/flash/provision/install path, safe serial-monitor handling, and the
        acceptance tiers that keep a cross-build from being mistaken for a working device.
      </PageGoal>

      <h2>Exact board composition</h2>
      <table>
        <thead><tr><th>Area</th><th>Current PocketPi target</th></tr></thead>
        <tbody>
          <tr><td>Board</td><td>Waveshare <code>ESP32-P4-WIFI6-Touch-LCD-5</code></td></tr>
          <tr><td>Processor</td><td><code>ESP32-P4NRW32</code>, dual-core RISC-V up to 400 MHz plus an LP core up to 40 MHz</td></tr>
          <tr><td>Memory configuration</td><td>32 MB QIO Flash, 32 MB PSRAM at 200 MHz, 256 KB L2 cache</td></tr>
          <tr><td>Display</td><td>5-inch 720×1280 IPS, 2-lane MIPI-DSI, HX8394, RGB565, three framebuffers</td></tr>
          <tr><td>Touch</td><td>GT911 capacitive 5-point touch</td></tr>
          <tr><td>Wireless</td><td>ESP32-C6 companion over ESP-Hosted, providing Wi-Fi 6 and BLE 5</td></tr>
        </tbody>
      </table>
      <p>
        These are the board and firmware parameters used by the current host, not a generic list of
        everything the ESP32-P4 chip can support. The App-facing logical viewport is 720×1280.
        Compare the <a href="https://www.waveshare.com/product/iot-communication/short-range-wireless/esp32-p4-wifi6-touch-lcd-5.htm" target="_blank" rel="noreferrer">Waveshare board specification</a> and
        the <a href="https://documentation.espressif.com/esp32-p4_datasheet_en.html" target="_blank" rel="noreferrer">Espressif ESP32-P4 datasheet</a> with the checked-in host configuration.
      </p>

      <h2>Toolchain</h2>
      <ul>
        <li><a href="https://rust-lang.org/tools/install/" target="_blank" rel="noreferrer">Rust with rustup</a>;</li>
        <li>the <a href="https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html" target="_blank" rel="noreferrer">esp-rs/ESP-IDF development environment</a>;</li>
        <li>Rust <code>nightly-2026-05-01</code> used by the repository wrapper;</li>
        <li><a href="https://github.com/esp-rs/espflash/blob/main/espflash/README.md" target="_blank" rel="noreferrer"><code>espflash</code></a> for port discovery, flashing and boot diagnostics;</li>
        <li>a PocketJS checkout only when regenerating the shared View SDK.</li>
      </ul>
      <p>
        The refactored Python UART tools use POSIX serial APIs. macOS is the documented host path;
        they are not currently a native Windows workflow.
      </p>

      <h2>Find the board serial port</h2>
      <Code>{`espflash list-ports
export DEVICE_PORT=/dev/cu.usbmodem...
espflash board-info --port "$DEVICE_PORT"`}</Code>
      <p>
        Choose the WCH USB serial device reported by <code>espflash</code>. Keep the variable in the
        same shell for the flash, provisioning, bridge and install commands below.
      </p>

      <h2>Build the release firmware</h2>
      <Code>{`cargo xtask build esp32-p4`}</Code>
      <p>
        This builds the ESP32-P4 release firmware with the generated Pi Agent bundle and View SDK
        resources committed to the repository. It does not regenerate either asset. Ordinary Apps
        are not embedded in the firmware image.
      </p>

      <h2>Flash without erasing persistent configuration</h2>
      <Code>{`espflash flash --baud 921600 --port "$DEVICE_PORT" \\
  --partition-table firmware/esp32-p4/partitions.csv \\
  firmware/esp32-p4/target/riscv32imafc-esp-espidf/release/pocket-pi-p4`}</Code>
      <p>
        A normal flash preserves NVS and existing LittleFS state. Do not erase the board unless a
        test explicitly requires destructive reprovisioning.
      </p>

      <h2>Provision standalone model and Wi-Fi configuration</h2>
      <Code>{`python3 tools/uart-provision.py "$DEVICE_PORT" \\
  --provider deepseek --provision-wifi`}</Code>
      <p>
        For DeepSeek, the tool reads account <code>deepseek-api-key</code> from the macOS Keychain
        service <code>Pocket Pi Credentials</code> when available; otherwise it prompts without echo.
        Other providers prompt for their key. Model provider, model, thinking level and API key are
        stored in native NVS. Wi-Fi uses the device&apos;s Wi-Fi NVS store. Normal boots load both stores
        without a Mac or UART bridge.
      </p>
      <Code>{`python3 tools/uart-provision.py "$DEVICE_PORT" \\
  --provider openai --model gpt-5-mini`}</Code>

      <h2>Development-only model bridge</h2>
      <Code>{`python3 tools/uart-model-bridge.py "$DEVICE_PORT" \\
  --provider codex \\
  --prompt "Use write, read, schedule.set and schedule.list."`}</Code>
      <p>
        Use this only for bring-up on an unprovisioned development board. It does not become the
        device&apos;s stored standalone backend and is not part of normal startup.
      </p>

      <h2>Install an ordinary App</h2>
      <Code>{`cargo xtask package app exa path/to/exa-credentials.json

python3 tools/uart-install.py "$DEVICE_PORT" \\
  target/pocketapps/exa.pocketapp`}</Code>
      <p>
        Wait for the review screen and confirm on the touch display. The same package can be uploaded
        through <code>http://&lt;device-ip&gt;/</code> when local peer access is available.
      </p>

      <h2>Serial monitor warning</h2>
      <p>
        On the board&apos;s WCH USB bridge, <code>espflash monitor</code> controls DTR/RTS. Opening it
        between App upload and physical confirmation can reset the board and discard the pending
        review. Use it for boot diagnostics, then restore a normal boot:
      </p>
      <Code>{`espflash monitor --port "$DEVICE_PORT"
espflash reset --port "$DEVICE_PORT" --non-interactive`}</Code>

      <h2>Physical acceptance checklist</h2>
      <ol>
        <li>Cold boot reaches the resident Agent Root View.</li>
        <li>LCD, touch and keyboard input work at the 720×1280 logical viewport.</li>
        <li>Saved Wi-Fi associates and receives DHCP without a Mac bridge.</li>
        <li>A fresh provider prompt returns through the configured standalone backend.</li>
        <li>Workspace write/read and Agent wake persistence survive restart.</li>
        <li>An App installs after touch review, exposes its Tool, writes SQLite and renders its View.</li>
        <li>An update preserves data/credentials; uninstall removes all App-owned state.</li>
      </ol>
      <p>Board-specific source and caveats: <SourceLink path="docs/esp32-p4-port.md" />.</p>
    </>,
  },
  {
    slug: "esp32-s3",
    title: "ESP32-S3 supported target",
    description: "Build, flash, provision and validate PocketPi on the Waveshare ESP32-S3-Touch-LCD-4.3.",
    render: () => <>
      <h1>ESP32-S3 supported target</h1>
      <DocLead>
        The Waveshare ESP32-S3-Touch-LCD-4.3 is PocketPi&apos;s second supported physical target.
        It runs the same resident Pi Agent, ordinary App packages, AppSupervisor, Actions and View
        source as the ESP32-P4 reference target through a shared ESP-IDF host layer.
      </DocLead>
      <PageGoal>
        The exact board contract, build and flash commands, viewport behavior, physical evidence and
        the limits that still require long-running hardware acceptance.
      </PageGoal>

      <h2>Exact board composition</h2>
      <table>
        <thead><tr><th>Area</th><th>Current PocketPi target</th></tr></thead>
        <tbody>
          <tr><td>Board</td><td>Waveshare <code>ESP32-S3-Touch-LCD-4.3</code></td></tr>
          <tr><td>Module</td><td><code>ESP32-S3-WROOM-1-N16R8</code></td></tr>
          <tr><td>Processor</td><td>dual-core Xtensa LX7 up to 240 MHz, 512 KB SRAM and 384 KB ROM</td></tr>
          <tr><td>Memory configuration</td><td>16 MB DIO Flash, 8 MB octal PSRAM at 80 MHz</td></tr>
          <tr><td>Display</td><td>4.3-inch 800×480 IPS RGB panel, RGB565, two PSRAM framebuffers</td></tr>
          <tr><td>Touch</td><td>GT911 capacitive 5-point touch over I2C</td></tr>
          <tr><td>Wireless</td><td>integrated 2.4 GHz 802.11 b/g/n Wi-Fi and BLE 5</td></tr>
        </tbody>
      </table>
      <p>
        Board-level parameters are also documented in the official
        <a href="https://docs.waveshare.com/ESP32-S3-Touch-LCD-4.3" target="_blank" rel="noreferrer"> Waveshare ESP32-S3-Touch-LCD-4.3 guide</a>.
        PocketPi-specific flash mode, PSRAM speed, framebuffer and viewport values come from the
        checked-in S3 firmware host.
      </p>

      <h2>One physical panel, one rotated logical viewport</h2>
      <p>
        The RGB panel scans out at 800×480. PocketPi rotates rendered regions into that framebuffer
        and maps touch through the inverse transform, then reports <code>Viewport(480, 800)</code> to
        AppSupervisor. Every View therefore sees one consistent logical coordinate system.
      </p>
      <Code>{`View.viewport
// {
//   width: 480,
//   height: 800,
//   orientation: "portrait",
//   scale: 0.625,
//   layoutWidth: 768,
//   layoutHeight: 1280
// }`}</Code>
      <p>
        The shared View SDK scales numeric geometry once, preserves fixed physical font slots and
        enforces at least 40×40 physical pixels for Pressable hit targets. Apps may branch on
        <code>orientation</code> and reduce repeated content when <code>scale</code> is below one; they
        do not receive a board name and must not contain S3-only layout code.
      </p>

      <h2>Install host tools and find the serial port</h2>
      <p>
        Install <a href="https://rust-lang.org/tools/install/" target="_blank" rel="noreferrer">Rust with rustup</a>,
        the <a href="https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html" target="_blank" rel="noreferrer">ESP Rust toolchain</a> and
        <a href="https://github.com/esp-rs/espflash/blob/main/espflash/README.md" target="_blank" rel="noreferrer"><code>espflash</code></a>.
        The repository invokes the S3 firmware through its declared <code>esp</code> toolchain. The
        current Python UART helpers use POSIX serial APIs; macOS is the documented host path.
      </p>
      <Code>{`espflash list-ports
export DEVICE_PORT=/dev/cu.usbmodem...
espflash board-info --port "$DEVICE_PORT"`}</Code>

      <h2>Build the release firmware</h2>
      <Code>{`cargo xtask build esp32-s3`}</Code>
      <p>
        This builds the S3 release firmware with the committed generated Pi Agent and View SDK
        assets. Regenerate those assets separately only when changing their sources.
      </p>

      <h2>Flash without erasing persistent state</h2>
      <Code>{`espflash flash --baud 921600 --port "$DEVICE_PORT" \\
  --partition-table firmware/esp32-s3/partitions.csv \\
  firmware/esp32-s3/target/xtensa-esp32s3-espidf/release/pocket-pi-s3`}</Code>
      <p>
        Keep the explicit partition table. A normal flash preserves Wi-Fi/model NVS and the LittleFS
        workspace. Do not erase the board unless the validation plan explicitly requires destructive
        reprovisioning.
      </p>

      <h2>Provision and install</h2>
      <Code>{`python3 tools/uart-provision.py "$DEVICE_PORT" \\
  --provider deepseek --provision-wifi

cargo xtask package app exa path/to/exa-credentials.json
python3 tools/uart-install.py "$DEVICE_PORT" \\
  target/pocketapps/exa.pocketapp`}</Code>
      <p>
        Provisioning, HTTP/UART ingress, product review and activation use the same contracts as P4.
        The board-specific host owns integrated Wi-Fi, RGB scanout, rotation and GT911 touch only.
      </p>

      <h2>Recorded physical evidence</h2>
      <p>
        The 2026-08-24 repository record covers boot, 480×800 logical scanout, GT911 touch,
        integrated Wi-Fi, workspace Tool Calls, ordinary App installation and an Exa request on the
        physical S3. Long-running latency, display stability and memory-pressure acceptance remain
        separate work and are not implied by one successful interaction.
      </p>
      <Fact>
        A simulator run or successful S3 release build is below physical-board acceptance. Record
        boot, scanout, touch, network, Tool and App results separately.
      </Fact>
      <p>Shared port contract and S3 details: <SourceLink path="docs/esp32-p4-port.md" />.</p>
    </>,
  },
];
