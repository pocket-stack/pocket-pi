import type { Metadata } from "next";
import { SiteFooter, SiteHeader } from "../site-chrome";

export const metadata: Metadata = {
  title: "Changelog · PocketPi",
  description: "PocketPi milestones derived from merged pocket-stack/pocket-pi pull requests.",
};

const releases = [
  {
    date: "August 24, 2026",
    iso: "2026-08-24",
    pr: 17,
    title: "Let Pi Agent iterate installed Apps",
    summary: "The resident Agent can now checkout an installed ordinary App, edit its source and submit the candidate through the existing physically confirmed update path.",
    changes: [
      "Add app.checkout and app.submit lifecycle Tools without adding a second transport or activation path.",
      "Keep editable source under apps/<id>/checkout while excluding App data, temporary files and credentials.",
      "Return bounded install/update outcomes from .system/app-events/<id>.json so an Agent can inspect the previous result before retrying.",
      "Keep the running release unchanged until validation, on-device review and physical confirmation complete.",
    ],
  },
  {
    date: "August 24, 2026",
    iso: "2026-08-24",
    pr: 20,
    title: "Refine compact chat and workspace file management",
    summary: "The Pi Agent System App gained denser viewport-aware Chat and Files surfaces plus bounded file deletion through an Action.",
    changes: [
      "Adapt compact Chat, Files, Apps and Settings layouts while keeping product policy in JavaScript.",
      "Route workspace.delete and the Files UI through the same Pi Agent deleteFile Action.",
      "Protect system roots and keep file mutation inside the System App data.fs capability.",
      "Clarify the firmware-seeded System App release lifecycle separately from ordinary App releases.",
    ],
  },
  {
    date: "August 23, 2026",
    iso: "2026-08-23",
    pr: 19,
    title: "Reduce ESP32-S3 display contention during Agent turns",
    summary: "The S3 host reduced avoidable scanout pressure while the Agent and wireless provider path are active.",
    changes: [
      "Tune the S3 runtime and display path around the shared framebuffer completion contract.",
      "Reduce model-turn display contention without moving UI policy into board firmware.",
      "Harden OpenAI-compatible and Anthropic response handling used by embedded wireless providers.",
      "Keep long-running display stability and memory-pressure work as separate physical acceptance.",
    ],
  },
  {
    date: "August 22, 2026",
    iso: "2026-08-22",
    pr: 18,
    title: "Add ESP32-S3 and a shared viewport contract",
    summary: "PocketPi added the Waveshare ESP32-S3-Touch-LCD-4.3 as a second supported target while preserving one App source and one AgentOS stack.",
    changes: [
      "Add firmware/esp32-s3 and extract shared ESP-IDF AgentOS, storage, Wi-Fi and transport mechanisms into firmware/esp32-common.",
      "Pass host logical viewports into every View Guest and expose width, height, orientation, scale and layout extents through View.viewport.",
      "Render the S3 480×800 logical surface into its rotated 800×480 RGB panel and map GT911 touch through the same transform.",
      "Make Pi Agent, Exa and Robinhood choose portrait stacks or landscape columns without board-specific App forks.",
    ],
  },
  {
    date: "August 18, 2026",
    iso: "2026-08-18",
    pr: 16,
    title: "Support ordinary App updates",
    summary: "Installed ordinary Apps can update through the same staging, review and physical-confirmation path used for first install.",
    changes: [
      "Preserve App SQLite data and native credentials across code-only and schema updates.",
      "Add forward migrations/N.sql, candidate rehearsal, one live migration transaction and atomic source activation.",
      "Refresh Tools, schedules, Action runtime and View runtime after activation.",
      "Complete an approved interrupted update during boot; release history and rollback remain out of scope.",
    ],
  },
  {
    date: "August 17, 2026",
    iso: "2026-08-17",
    pr: 15,
    title: "Enable source-loaded Apps",
    summary: "Ordinary Apps moved to raw JavaScript and SQL while preserving App = Data + Actions + View.",
    changes: [
      "Add the minimal raw View SDK and source App runtime.",
      "Migrate Exa and Robinhood to source-loaded Actions, SQLite Projections and fixed Views.",
      "Add retained View reconciliation so native nodes are reused and only changed props or text are emitted.",
      "Validate source-App install and Exa/Robinhood interaction on physical ESP32-P4 hardware.",
    ],
  },
  {
    date: "August 15, 2026",
    iso: "2026-08-15",
    pr: 14,
    title: "Establish the bundle-first Agent-native runtime",
    summary: "The runtime established one PocketJS substrate, one JavaScript System Framework and the App = Data + Actions + View contract.",
    changes: [
      "Keep one resident Pi Agent System Guest plus independent three-entry View and Action Guest LRUs.",
      "Make Actions actor-neutral so Agent Tools, UI events and schedules share one behavior boundary.",
      "Move product policy into JavaScript while native code retains hardware, isolation, security and lifecycle mechanisms.",
      "Rename the ordinary package descriptor to app.json and remove compatibility artifacts.",
    ],
  },
  {
    date: "August 14, 2026",
    iso: "2026-08-14",
    pr: 13,
    title: "Complete the ordinary App lifecycle",
    summary: "Ordinary Apps gained destructive uninstall and UART ingress through the same AppSupervisor path as HTTP.",
    changes: [
      "Remove an uninstalled App's Tool routes, schedules, Guests, SQLite/data, credentials, native sessions and release.",
      "Protect the resident Pi Agent System App from uninstall.",
      "Persist standalone wireless model configuration in native NVS and remove the implicit UART model fallback.",
      "Exercise UART install, uninstall, restart absence and clean reinstall on the physical ESP32-P4.",
    ],
  },
  {
    date: "August 14, 2026",
    iso: "2026-08-14",
    pr: 12,
    title: "Install ordinary Apps at runtime",
    summary: "Firmware stopped embedding ordinary Apps; each App became one independently installable .pocketapp.",
    changes: [
      "Keep only the resident Pi Agent System App in Firmware.",
      "Add one native Installer contract and a review/installing/success/failure UI.",
      "Use local HTTP only as package ingress; lifecycle state remains owned by Installer.",
      "Allocate App QuickJS heaps and large ESP32 worker stacks from PSRAM through the host-selected allocator.",
    ],
  },
  {
    date: "August 13, 2026",
    iso: "2026-08-13",
    pr: 11,
    title: "Build Pocket Pi as a device-native, Agent-native runtime",
    summary: "The project adopted its complete embedded-device runtime boundary instead of maintaining a separate desktop product composition.",
    changes: [
      "Make Pi Agent the resident System App that owns /workspace.",
      "Define ordinary Apps through public Tools, private Data Actions and schedules, App-local SQLite and fixed PocketJS Views.",
      "Share AgentOS and PocketJS product contracts between ESP32-P4 and its macOS development simulator.",
      "Keep credentials, TLS, transport, hardware, lifecycle and timeout enforcement in native hosts.",
    ],
  },
];

const earlierReleases = [
  { date: "August 6, 2026", pr: 10, title: "docs: link PocketJS from README" },
  { date: "August 6, 2026", pr: 9, title: "feat: run Pocket Pi across macOS, simulator, and ESP32-P4" },
  { date: "July 22, 2026", pr: 8, title: "chore(pocket-pi): hello acceptance example" },
  { date: "July 22, 2026", pr: 7, title: "refactor(pocket-pi)!: full unmodified pi is the sole default; remove trimmed path" },
  { date: "July 22, 2026", pr: 6, title: "refactor(pocket-pi): typecheck the pi-full harness against pi's real API" },
  { date: "July 22, 2026", pr: 5, title: "chore: mark generated JS as linguist-generated" },
  { date: "July 22, 2026", pr: 4, title: "refactor(pocket-pi): TypeScript build pipeline for the JS guest layer" },
  { date: "July 22, 2026", pr: 3, title: "feat(pocket-pi): bump to pi 0.81 + self-contained embedded binary" },
  { date: "July 22, 2026", pr: 2, title: "docs(readme): rewrite around unmodified pi + extensions (M5 to M7)" },
  { date: "July 22, 2026", pr: 1, title: "feat(pocket-pi): run unmodified pi-coding-agent + extensions on QuickJS (M5 to M7)" },
];

export default function ChangelogPage() {
  const releaseDays = releases.filter((release, index) => (
    releases.findIndex((candidate) => candidate.iso === release.iso) === index
  ));

  return (
    <div className="site-shell subpage-shell">
      <SiteHeader active="changelog" />
      <main className="changelog-page">
        <header className="changelog-header">
          <p className="page-eyebrow">Shipped runtime history</p>
          <h1>Changelog</h1>
          <p>
            Runtime milestones derived from merged pull requests in <code>pocket-stack/pocket-pi</code>.
            Roadmap ideas and website-only edits are not listed as shipped runtime changes.
          </p>
        </header>

        <div className="changelog-stream">
          {releaseDays.map((day) => (
            <section className="changelog-day" key={day.iso}>
              <div className="changelog-date">
                <time dateTime={day.iso}>{day.date}</time>
              </div>
              <div className="changelog-day-releases">
                {releases.filter((release) => release.iso === day.iso).map((release) => (
                  <article className="release-note" key={release.pr}>
                    <div className="release-note-heading">
                      <h2>{release.title}</h2>
                      <a href={`https://github.com/pocket-stack/pocket-pi/pull/${release.pr}`} target="_blank" rel="noreferrer">PR #{release.pr} <span aria-hidden="true">↗</span></a>
                    </div>
                    <p className="release-summary">{release.summary}</p>
                    <ul className="release-change-grid">
                      {release.changes.map((change) => <li key={change}>{change}</li>)}
                    </ul>
                  </article>
                ))}
              </div>
            </section>
          ))}
        </div>

        <section className="earlier-history" aria-labelledby="earlier-history-title">
          <div>
            <p className="page-eyebrow">Foundation</p>
            <h2 id="earlier-history-title">Earlier merged history</h2>
            <p>The repository foundation, listed directly from merged PRs #1 through #10.</p>
          </div>
          <div className="earlier-history-grid">
            {earlierReleases.map((release) => (
              <a href={`https://github.com/pocket-stack/pocket-pi/pull/${release.pr}`} key={release.pr} target="_blank" rel="noreferrer">
                <time>{release.date}</time>
                <span><b>#{release.pr}</b> {release.title}</span>
                <i aria-hidden="true">↗</i>
              </a>
            ))}
          </div>
        </section>
      </main>
      <SiteFooter />
    </div>
  );
}
