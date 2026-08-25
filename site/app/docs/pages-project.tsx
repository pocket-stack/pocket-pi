import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink, Status } from "./doc-components";

export const PROJECT_DOCS: DocRecord[] = [
  {
    slug: "current-boundaries",
    title: "Current boundaries",
    description: "What is implemented on upstream/main, what is an explicit non-feature and what is a labeled future direction.",
    render: () => <>
      <h1>Current boundaries</h1>
      <DocLead>
        These statements describe <code>upstream/main</code> at commit
        <code>8b3866f</code> after PRs #17 through #20. Implemented behavior, validated evidence and future design
        are intentionally kept separate.
      </DocLead>
      <PageGoal>
        A reliable “can I use this today?” answer for App developers, host developers and people
        evaluating the project, without presenting a handoff or roadmap as shipped product.
      </PageGoal>

      <h2>Implemented</h2>
      <ul>
        <li><Status>Implemented</Status> Pi Agent is the firmware-embedded resident System App and owns the top-level workspace.</li>
        <li><Status>Implemented</Status> ESP32-P4 is the reference host and ESP32-S3 is a supported second physical host; macOS provides their shared product-contract simulator.</li>
        <li><Status>Implemented</Status> Ordinary Apps execute raw <code>app.json</code>, SQL and JavaScript source.</li>
        <li><Status>Implemented</Status> Ordinary Apps install through HTTP or UART after one shared product review.</li>
        <li><Status>Implemented</Status> Apps update source while preserving compatible SQLite data and native credentials.</li>
        <li><Status>Implemented</Status> Forward SQL migrations rehearse before live mutation and approved interrupted updates recover at boot.</li>
        <li><Status>Implemented</Status> Uninstall removes App routes, schedules, Guests, credentials, native session state, source and data.</li>
        <li><Status>Implemented</Status> Agent Tools, UI events and App schedules route to the same named Actions.</li>
        <li><Status>Implemented</Status> View and Action Guests use independent three-entry LRUs around one resident System Guest.</li>
        <li><Status>Implemented</Status> App commits drive revision-coalesced, bounded foreground Projections.</li>
        <li><Status>Implemented</Status> The shared View SDK exposes one host-provided viewport and composes responsive App interfaces from JavaScript <code>Row</code>, <code>Column</code> and shared components.</li>
        <li><Status>Implemented</Status> Pi Agent can checkout and edit an installed ordinary App, then submit it to the existing physically confirmed update path.</li>
      </ul>

      <h2>Not implemented</h2>
      <ul>
        <li>a replaceable resident Harness; Pi Agent is the only shipped Harness;</li>
        <li>conversation/session persistence across reboot;</li>
        <li>independent System App or System Framework update;</li>
        <li>release history, persistent rollback, downgrade or undelete;</li>
        <li>multi-file executable ES modules or on-device TypeScript/TSX/JSX transformation;</li>
        <li>a package dependency graph, third-party App plugin framework or marketplace;</li>
        <li>an Agent-exposed App install/update API that bypasses physical review;</li>
        <li>a generic desktop Agent OS product, CPU/peripheral emulator or web Playground.</li>
      </ul>

      <h2>Near-term design direction</h2>
      <ol>
        <li><Status>Research target</Status> Make the resident Harness build-time replaceable between Pi Agent and DeepSeek Harness.</li>
        <li>Reach product-experience parity before adopting DeepSeek Harness plugins or new session semantics.</li>
        <li>Keep App/runtime/host contracts stable while the guest-side adapter changes.</li>
        <li>After another real hardware/platform need exists, consider extracting the Pi Agent and DeepSeek Harness compatibility layers into independently reusable packages.</li>
        <li>Add a web Playground later by moving the macOS product simulator contract to the web, not by redefining the core App model.</li>
      </ol>
      <p>See <a href="/docs/harnesses">Harness boundary</a> for the parity contract and explicit deferrals.</p>

      <h2>Product non-goals that protect focus</h2>
      <ul>
        <li>Do not turn the native host into the place where each App&apos;s product logic lives.</li>
        <li>Do not make the model a required runtime for deterministic refresh, cleanup or rendering.</li>
        <li>Do not expose raw credentials or private App databases to the Agent for convenience.</li>
        <li>Do not call simulator or cross-build success physical-device acceptance.</li>
        <li>Do not add a broad plugin/dependency system before a concrete App proves the small contract insufficient.</li>
      </ul>
      <Fact>
        The website uses “PocketPi” as the product name and “Agent-native runtime for embedded
        devices” as the technical category. It does not claim a kernel or RTOS replacement.
      </Fact>
    </>,
  },
  {
    slug: "validation-status",
    title: "Validation status",
    description: "The latest checked-in evidence tiers for tests, simulator, firmware builds and physical ESP32-P4/ESP32-S3 behavior.",
    render: () => <>
      <h1>Validation status</h1>
      <DocLead>
        Validation is reported by evidence tier. Dates and counts below are the latest records checked
        into <code>upstream/main</code>; they are not a substitute for rerunning acceptance on a new commit,
        provider configuration or physical board.
      </DocLead>
      <PageGoal>
        A precise view of what current repository evidence proves, which physical results predate the
        latest Source App contract and what remains outside the evidence set.
      </PageGoal>

      <h2>Evidence ladder</h2>
      <Code>{`repository source
  → Rust/App contract tests
  → simulator end-to-end
  → ESP32-P4 and ESP32-S3 release builds
  → physical boot/display/touch/storage per board
  → fresh Wi-Fi/provider/App end-to-end`}</Code>

      <h2>Recorded on 2026-08-24</h2>
      <ul>
        <li>57 Rust tests completed without failure.</li>
        <li>workspace Clippy passed with warnings denied.</li>
        <li>Pi Agent assets, shared View SDK, simulator, ESP32-P4 release firmware and ESP32-S3 release firmware built.</li>
        <li>core tests covered viewport propagation and scaling, minimum touch targets, App lifecycle, SQLite ownership and model/Tool contracts.</li>
        <li>physical P4 coverage includes the Agent, workspace, schedules, App install/update/uninstall, display/touch, Wi-Fi and direct model providers.</li>
        <li>physical S3 coverage includes boot, 480×800 logical scanout, GT911 touch, integrated Wi-Fi, workspace Tool Calls, ordinary App install and an Exa request.</li>
      </ul>

      <h2>Evidence remains board-specific</h2>
      <p>
        P4 has the broader reference-target history, including update and uninstall. S3 physical
        evidence proves the shared runtime and source App contract on its current board composition,
        but it does not inherit P4-only stress or lifecycle results. Long-running latency, display
        stability and memory-pressure testing remain separate acceptance work on both targets.
      </p>

      <h2>Current automated contract coverage</h2>
      <ul>
        <li>View and Action LRUs retain the three most recent ordinary Guests.</li>
        <li>Tool routing selects each App&apos;s declared Action.</li>
        <li>App revisions coalesce at a foreground frame boundary.</li>
        <li>code-only update preserves data and native credentials.</li>
        <li>schema update rejects missing steps and preserves rows.</li>
        <li>boot finishes an approved update interrupted before activation.</li>
        <li>schedule success records only after the Action completes.</li>
        <li>Projection/SQLite errors propagate rather than becoming an empty View.</li>
        <li>uninstall removes App-owned routes, runtimes, schedules, credentials and data.</li>
      </ul>

      <h2>Not yet implied by those checks</h2>
      <ul>
        <li>phone upload on every network topology;</li>
        <li>a fresh live provider call for every current commit;</li>
        <li>unattended long-duration memory pressure;</li>
        <li>physical acceptance of a future DeepSeek Harness build;</li>
        <li>a future third-board port or a web Playground.</li>
      </ul>

      <h2>How to report a new result</h2>
      <p>
        Record the exact commit, build command, board/host, provisioned backend, whether storage was
        erased, the observable pass/failure and the highest completed tier. Keep failures such as
        Wi-Fi association or DHCP timeout attached to the network-dependent acceptance instead of
        calling a successful boot complete end-to-end validation.
      </p>
      <p>Checked-in evidence summary: <SourceLink path="README.md#current-validation">README current validation</SourceLink>.</p>
    </>,
  },
];
