import type { Metadata } from "next";
import { SiteFooter, SiteHeader } from "../../site-chrome";

const title = "Making Pocket Pi an Agent-Native App Development Environment";
const description =
  "How PocketPi gives Pi Agent an Inspect, Modify, Validate and Commit loop for Apps whose source, state and lifecycle live on an ESP32-S3.";

export const metadata: Metadata = {
  title: `${title} · PocketPi Blog`,
  description,
  openGraph: {
    type: "article",
    title,
    description,
    publishedTime: "2026-08-25T00:00:00+08:00",
    authors: ["Siwei \"Jerry\" Yuan"],
    images: [
      {
        url: "/blog/agent-native-app-development-environment/physical-before-after.png",
        width: 1080,
        height: 1440,
        alt: "The same PocketPi App before and after Pi Agent updated its Action and View on an ESP32-S3",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title,
    description,
    images: ["/blog/agent-native-app-development-environment/physical-before-after.png"],
  },
};

function DevelopmentLoopDiagram() {
  return (
    <svg
      className="development-loop-diagram"
      viewBox="0 0 760 540"
      width="100%"
      role="img"
      aria-labelledby="development-loop-title development-loop-description"
      fontFamily="ui-monospace,SFMono-Regular,Menlo,monospace"
    >
      <title id="development-loop-title">PocketPi minimal on-device development loop</title>
      <desc id="development-loop-description">
        Pi Agent inspects ordinary App source from its privileged workspace, modifies an isolated
        checkout, validates it through human review and rehearsal, and commits one coherent App
        version.
      </desc>
      <rect width="760" height="540" rx="12" fill="#080c14" />
      <text x="20" y="32" fill="#718198" fontSize="12">POCKETPI / MINIMAL ON-DEVICE DEVELOPMENT LOOP</text>

      <rect x="18" y="58" width="160" height="262" rx="10" fill="#0c151e" stroke="#d49a36" strokeWidth="1.5" />
      <text x="34" y="86" fill="#d49a36" fontSize="10" fontWeight="700">INSPECT</text>
      <text x="34" y="113" fill="#e8eef7" fontSize="16" fontWeight="700">Guest</text>
      <text x="34" y="133" fill="#e8eef7" fontSize="16" fontWeight="700">hierarchy</text>
      <line x1="34" y1="150" x2="162" y2="150" stroke="#263248" />
      <text x="34" y="172" fill="#cbd6e2" fontSize="9">Pi Agent → workspace</text>
      <text x="34" y="192" fill="#8f9db0" fontSize="9">Apps → own Data only</text>
      <rect x="34" y="198" width="128" height="48" rx="6" fill="#111c2a" />
      <text x="48" y="217" fill="#b8c5d3" fontSize="9">read · find</text>
      <text x="48" y="235" fill="#b8c5d3" fontSize="9">grep · ls</text>
      <rect x="34" y="267" width="128" height="31" rx="6" fill="#151b22" stroke="#d49a36" strokeOpacity=".55" />
      <text x="98" y="287" fill="#d8b774" fontSize="8" textAnchor="middle">VISIBLE, STILL ISOLATED</text>

      <path d="M184 187 H202" stroke="#536981" strokeWidth="1.5" />
      <path d="M195 181 L202 187 L195 193" fill="none" stroke="#7e95ad" strokeWidth="1.5" />

      <rect x="206" y="58" width="160" height="262" rx="10" fill="#0f1623" stroke="#5da8d1" strokeWidth="1.5" />
      <text x="222" y="86" fill="#5da8d1" fontSize="10" fontWeight="700">MODIFY</text>
      <text x="222" y="113" fill="#e8eef7" fontSize="16" fontWeight="700">App checkout</text>
      <text x="222" y="133" fill="#8f9db0" fontSize="9">release → candidate</text>
      <line x1="222" y1="150" x2="350" y2="150" stroke="#263248" />
      <text x="222" y="174" fill="#cbd6e2" fontSize="9">app.json</text>
      <text x="222" y="193" fill="#a9d9c6" fontSize="9">schema · migrations</text>
      <text x="222" y="212" fill="#e7c27e" fontSize="9">actions.js</text>
      <text x="294" y="212" fill="#c1b3e7" fontSize="9">view.js</text>
      <rect x="222" y="230" width="128" height="36" rx="6" fill="#101d2a" stroke="#5da8d1" strokeOpacity=".65" />
      <text x="286" y="252" fill="#9bcdea" fontSize="8" textAnchor="middle">WRITE · EDIT</text>
      <text x="286" y="289" fill="#8f9db0" fontSize="9" textAnchor="middle">live App unchanged</text>

      <path d="M372 187 H390" stroke="#536981" strokeWidth="1.5" />
      <path d="M383 181 L390 187 L383 193" fill="none" stroke="#7e95ad" strokeWidth="1.5" />

      <rect x="394" y="58" width="160" height="262" rx="10" fill="#0f1623" stroke="#9f8bd8" strokeWidth="1.5" />
      <text x="410" y="86" fill="#9f8bd8" fontSize="10" fontWeight="700">VALIDATE</text>
      <text x="410" y="113" fill="#e8eef7" fontSize="16" fontWeight="700">Review +</text>
      <text x="410" y="133" fill="#e8eef7" fontSize="16" fontWeight="700">rehearse</text>
      <line x1="410" y1="150" x2="538" y2="150" stroke="#263248" />
      <circle cx="421" cy="168" r="8" fill="#171c31" stroke="#9f8bd8" />
      <text x="421" y="171" fill="#c1b3e7" fontSize="7" textAnchor="middle">1</text>
      <text x="437" y="171" fill="#cbd6e2" fontSize="8">app.submit</text>
      <circle cx="421" cy="199" r="8" fill="#171c31" stroke="#9f8bd8" />
      <text x="421" y="202" fill="#c1b3e7" fontSize="7" textAnchor="middle">2</text>
      <text x="437" y="202" fill="#cbd6e2" fontSize="8">human review</text>
      <circle cx="421" cy="230" r="8" fill="#171c31" stroke="#9f8bd8" />
      <text x="421" y="233" fill="#c1b3e7" fontSize="7" textAnchor="middle">3</text>
      <text x="437" y="233" fill="#cbd6e2" fontSize="8">SQLite rehearsal</text>
      <circle cx="421" cy="261" r="8" fill="#171c31" stroke="#9f8bd8" />
      <text x="421" y="264" fill="#c1b3e7" fontSize="7" textAnchor="middle">4</text>
      <text x="437" y="264" fill="#cbd6e2" fontSize="8">load Actions + View</text>
      <text x="474" y="294" fill="#9f8bd8" fontSize="8" textAnchor="middle">NO LIVE MUTATION</text>

      <path d="M560 187 H578" stroke="#536981" strokeWidth="1.5" />
      <path d="M571 181 L578 187 L571 193" fill="none" stroke="#7e95ad" strokeWidth="1.5" />

      <rect x="582" y="58" width="160" height="262" rx="10" fill="#0b1717" stroke="#65c3a0" strokeWidth="1.5" />
      <text x="598" y="86" fill="#65c3a0" fontSize="10" fontWeight="700">COMMIT</text>
      <text x="598" y="113" fill="#e8eef7" fontSize="16" fontWeight="700">One App</text>
      <text x="598" y="133" fill="#e8eef7" fontSize="16" fontWeight="700">version</text>
      <line x1="598" y1="150" x2="726" y2="150" stroke="#263248" />
      <rect x="598" y="166" width="128" height="31" rx="6" fill="#10201e" />
      <text x="662" y="186" fill="#add9c7" fontSize="9" fontWeight="700" textAnchor="middle">LIVE MIGRATION</text>
      <rect x="598" y="205" width="128" height="31" rx="6" fill="#10201e" />
      <text x="662" y="225" fill="#add9c7" fontSize="9" fontWeight="700" textAnchor="middle">RELEASE RENAME</text>
      <rect x="598" y="244" width="128" height="31" rx="6" fill="#10201e" />
      <text x="662" y="264" fill="#add9c7" fontSize="9" fontWeight="700" textAnchor="middle">NEW GUESTS</text>
      <text x="662" y="293" fill="#8f9db0" fontSize="8" textAnchor="middle">Tools + Schedules refresh</text>

      <rect x="18" y="344" width="724" height="68" rx="9" fill="#0c1622" stroke="#65c3a0" strokeWidth="1.25" />
      <text x="36" y="369" fill="#65c3a0" fontSize="10" fontWeight="700">DURABLE APP DATA</text>
      <text x="36" y="391" fill="#c9d5df" fontSize="11">Current SQLite stays outside release/ and checkout/</text>
      <path d="M450 379 H706" stroke="#65c3a0" strokeWidth="1.5" />
      <path d="M698 373 L706 379 L698 385" fill="none" stroke="#8ad2b7" strokeWidth="1.5" />
      <text x="578" y="368" fill="#718f87" fontSize="8" textAnchor="middle">REHEARSAL COPY → LIVE TRANSACTION</text>

      <rect x="18" y="432" width="724" height="48" rx="8" fill="#111522" stroke="#9f8bd8" strokeWidth="1.25" />
      <text x="380" y="461" fill="#d2c8eb" fontSize="10" fontWeight="700" textAnchor="middle">PocketJS / one substrate, one resident System Guest, many isolated App Guests</text>
      <rect x="18" y="498" width="724" height="30" rx="7" fill="#111821" stroke="#53637a" strokeWidth="1.25" />
      <text x="380" y="518" fill="#9eacbd" fontSize="9" textAnchor="middle">Native host / workspace bounds · SQLite ownership · lifecycle · crash recovery</text>
    </svg>
  );
}

export default function AgentNativeDevelopmentEnvironmentArticle() {
  return (
    <div className="site-shell subpage-shell">
      <SiteHeader active="blog" />
      <main className="blog-article">
        <header className="blog-article-header">
          <a className="blog-back-link" href="/blog">← Blog</a>
          <p className="page-eyebrow">PocketPi architecture</p>
          <h1>{title}</h1>
          <p className="blog-article-deck">
            An ESP32-S3 cannot host a conventional JavaScript toolchain. PocketPi becomes a
            development environment by closing a smaller, stricter loop around the App already
            running on the board.
          </p>
          <div className="blog-article-byline">
            <span>Siwei &quot;Jerry&quot; Yuan</span>
            <time dateTime="2026-08-25">August 25, 2026</time>
            <span>7 min read</span>
          </div>
        </header>

        <article className="blog-article-content">
          <section>
            <p className="article-opening">
              The ESP32-S3 used for this project has a 240 MHz CPU and 8 MB of PSRAM. It cannot
              run Node.js, npm, a TypeScript compiler, an unrestricted shell and an IDE in the
              way a Mac or Windows PC can.
            </p>
            <p>
              Instead of reproducing that desktop environment, we started from a first-principles
              question:
            </p>
            <blockquote>
              Given PocketJS as a small but complete JavaScript runtime substrate, what is the
              minimal closed loop required for an Agent to develop software?
            </blockquote>
            <p>The answer is four responsibilities:</p>
            <ol>
              <li>inspect the source that defines the running App;</li>
              <li>modify that source without mutating the live App;</li>
              <li>validate the candidate against the App&apos;s current Data;</li>
              <li>commit it as one coherent new App version.</li>
            </ol>
            <p>
              A compiler, a POSIX shell and an IDE are possible ways to implement those
              responsibilities. They are not the definition of a development environment. For
              PocketPi, the rest of the architecture exists to make these four steps complete on
              the device itself.
            </p>
            <p>
              The model backend may run elsewhere. Here, <em>on-device</em> means the App source,
              workspace Tools, durable state, validation and activation all live on the target
              that runs the App.
            </p>
            <p>
              Two earlier decisions supplied the foundation. In{" "}
              <a href="https://pocketjs.dev/blog/agent-native-runtime-embedded-systems/">
                Taking a Step Further Towards an Agent-Native Runtime on Embedded Systems
              </a>
              , we separated protected native mechanisms from editable JavaScript policy. In{" "}
              <a href="https://pocketjs.dev/blog/pocket-pi-agent-native-runtime/">
                Designing Apps for Humans and Agents in an Agent-Native Runtime
              </a>
              , we defined the editable product boundary as:
            </p>
            <pre><code>App = Data + Actions + View</code></pre>
            <p>
              The development loop works because each of its four steps maps back to one of those
              architecture choices.
            </p>
          </section>
        </article>

        <figure className="blog-article-figure architecture-figure">
          <DevelopmentLoopDiagram />
          <figcaption>
            The four responsibilities stay distinct. Human review and runtime rehearsal are the
            two validation gates before commit.
          </figcaption>
        </figure>

        <article className="blog-article-content">
          <section>
            <h2>Inspect: make the running App legible</h2>
            <p>
              An Agent cannot evolve software it can only observe as pixels. The first requirement
              is therefore not an editing Tool. It is a filesystem model that makes App source
              visible without dissolving the boundaries between Apps.
            </p>
            <pre><code>{`/workspace/
├── system/app/                 resident Pi Agent System App
└── apps/
    ├── demo/
    │   ├── release/            source currently running
    │   ├── data/               Demo-owned SQLite and files
    │   └── tmp/                disposable Demo files
    └── research/
        ├── release/            separate App source
        └── data/               separate App-owned state`}</code></pre>
            <p>
              Ordinary Apps are isolated Guests. An ordinary View or Action Guest receives only
              the database and filesystem surface owned by that App. It cannot walk into a peer
              App or the native host. Source and durable Data are also separate, so replacing
              source does not grant a new release ownership of another App&apos;s state.
            </p>
            <p>
              Pi Agent sits one level above those ordinary Guests. It is the firmware-embedded,
              resident System App, and the native host gives its workspace Tools a bounded view of
              <code>/workspace</code>. That is why Pi Agent can use <code>read</code>, <code>find</code>,{" "}
              <code>grep</code> and <code>ls</code> across ordinary App source while each ordinary App
              remains confined to its own surfaces. Path resolution rejects escapes beyond the
              workspace, and native credentials remain outside the source tree.
            </p>
            <p>
              Inspection therefore needs no checkout. The running definition is already legible
              in <code>apps/&lt;id&gt;/release</code>. Checkout is needed only when Pi Agent intends to
              change that definition.
            </p>
          </section>
        </article>

        <figure className="blog-article-figure simulator-figure">
          <div className="simulator-frame-grid">
            <article>
              <span>Resident Agent</span>
              {/* Deterministic output from the real ESP32 simulator. */}
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src="/pocketpi-device/screens/main.png" alt="PocketPi Simulator Chat screen with the resident Pi Agent" />
            </article>
            <article>
              <span>Visible workspace</span>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src="/pocketpi-device/screens/files.png" alt="PocketPi Simulator Files screen showing the ESP32 workspace" />
            </article>
            <article>
              <span>Human update gate</span>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src="/blog/agent-native-app-development-environment/update-review.png"
                alt="PocketPi Simulator review screen for updating Demo from version 1.0.0 to 1.1.0"
              />
            </article>
          </div>
          <figcaption>
            Three 480 × 800 frames from the real ESP32 product-contract simulator. In the third,
            Pi Agent has submitted a complete update, but activation still waits for human
            confirmation on the device.
          </figcaption>
        </figure>

        <article className="blog-article-content">
          <section>
            <h2>Modify: edit a bounded candidate</h2>
            <p>
              Reading live source is safe; editing it in place is not. The first App-iteration
              lifecycle boundary is therefore <code>app.checkout({`{ id }`})</code>. It copies the
              complete installed <code>release/</code> source once into an isolated{" "}
              <code>checkout/</code> and returns that canonical path. Calling it again reopens the
              same candidate instead of overwriting work already in progress.
            </p>
            <p>
              Checkout copies source only. The live SQLite database, temporary files and native
              credentials stay where they are. This creates the smallest useful write boundary:
              Pi Agent can change the complete App definition without mutating the running App or
              cloning its live state.
            </p>
            <p>The candidate is small enough to understand as a directory, not as a build graph:</p>
            <pre><code>{`apps/demo/checkout/
├── app.json                 identity, version, capabilities, Tools, Schedules
├── schema.sql               initial SQLite shape for a new installation
├── migrations/N.sql        forward Data changes for an existing installation
├── actions.js               actor-neutral behavior and SQLite writes
├── view.js                  human UI assembled with the shared View SDK
└── assets/*.json            manifest-declared static resources`}</code></pre>
            <p>
              The existing bounded <code>write</code> and exact-replacement <code>edit</code> Tools do
              the actual modification. Pi Agent advances the App version in <code>app.json</code>,
              changes behavior in <code>actions.js</code>, and changes presentation in{" "}
              <code>view.js</code>. If the durable data shape changes, it also advances{" "}
              <code>schemaVersion</code> and adds the next migration. If only values or behavior
              change, the schema version stays put.
            </p>
            <p>
              In the physical demo, those edits were concrete. The Action changed what a button
              writes to SQLite, while the View changed the badge from blue to green for the new
              value. The same candidate could also have included a migration if the SQLite shape
              had changed.
            </p>
            <p>
              Both entrypoints are raw JavaScript. <code>actions.js</code> uses the shared Framework
              to define Actions and Data access. <code>view.js</code> assembles{" "}
              <code>View.Screen</code>, <code>View.Row</code>, <code>View.Text</code>,{" "}
              <code>View.Badge</code> and <code>View.ActionButton</code> from the shared View SDK.
              Layout, semantic colors, typography, touch targets and viewport behavior live in
              that SDK, so Pi Agent describes the UI with a bounded vocabulary instead of
              reimplementing a renderer for each App.
            </p>
            <p>
              There is no on-device TypeScript transform, npm installation or cross-module
              compile. The files Pi Agent edits are already the files PocketJS will evaluate. The
              second lifecycle boundary, <code>app.submit</code>, begins only after that candidate is
              ready to be validated.
            </p>
          </section>
        </article>

        <article className="blog-article-content">
          <section>
            <h2>Validate: review and rehearse the candidate</h2>
            <p>
              A candidate is not valid merely because its JavaScript parses. The requested change
              must make sense to the human who owns the device, and the implementation must work
              against the state accumulated by the App already running there.
            </p>
            <p>
              <code>app.submit({`{ path }`})</code> first verifies that Pi Agent is submitting the
              canonical checkout for that App. It validates the manifest identity, source layout,
              App version, Framework API and schema-update contract, then stages the complete
              candidate as an update request.
            </p>
            <p>
              That request is deliberately human-in-the-loop. The user reviews what Pi Agent is
              asking to replace and either confirms it or rejects it and asks for a revision. The
              human answers the product question: is this the change we want? Native validation
              answers a different question: can this candidate safely become the running App?
            </p>
            <p>
              After confirmation, PocketPi performs a rehearsal before any live mutation. This is
              possible because durable App state has one explicit owner: an App-local SQLite
              database outside every View and Action Guest heap. The updater copies the quiescent
              database into a rehearsal directory, applies the candidate migrations to that copy,
              and loads the candidate Actions and View against the rehearsed Data.
            </p>
            <p>
              Missing migration steps, invalid Action routes, Framework errors and View
              construction failures stop the update while the installed release and its Data
              remain untouched. Source-only updates keep the same <code>schemaVersion</code>. A real
              SQLite shape change supplies the corresponding <code>migrations/N.sql</code> steps,
              while PocketPi owns the transaction and SQLite <code>user_version</code>.
            </p>
          </section>

          <section>
            <h2>Commit: activate one coherent App version</h2>
            <p>
              Once the human has approved the request and the rehearsal has succeeded, commit
              advances the whole App. It does not copy individual edited files over the running
              directory.
            </p>
            <p>
              The updater moves the complete candidate under <code>.update/release</code>, then
              applies the rehearsed migrations to the live SQLite database in one transaction.
              It quiesces the old Action and View runtimes, preserves the old source temporarily,
              and uses same-filesystem directory renames to place the complete candidate at the
              one canonical <code>release/</code> path.
            </p>
            <p>
              Atomicity follows the actual ownership boundaries. SQLite migration is atomic in a
              database transaction. Source activation uses complete-directory renames rather than
              partial file writes. The <code>.update</code> directory is a crash-recovery record, so
              a reboot can finish an interrupted activation instead of inventing a mixed release.
            </p>
            <p>
              This source switch can become executable immediately because PocketJS is one
              substrate designed to host multiple isolated Guests. The old cached Guests are
              discarded. A fresh Action Guest evaluates the platform-owned System Framework and
              then the new raw <code>actions.js</code>; a fresh View Guest evaluates the shared View
              SDK and then the new raw <code>view.js</code>. No module graph is compiled and no
              Firmware is rebuilt.
            </p>
            <p>
              Finally, PocketPi replaces the App&apos;s Tool routes and Schedules, publishes the new
              catalog entry and removes the temporary old source. The App id, native credentials
              and SQLite owner remain stable. Data is preserved or migrated; Actions and View are
              replaced; runtime Guests are recreated. That is one coherent transition of{" "}
              <code>Data + Actions + View</code> rather than a set of unrelated patches.
            </p>
          </section>
        </article>

        <figure className="blog-article-figure physical-result-figure">
          {/* The image is an editorial summary made from the physical ESP32-S3 recording. */}
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/blog/agent-native-app-development-environment/physical-before-after.png"
            alt="The same Demo App before and after Pi Agent updated it directly on ESP32-S3: SQLite Data is preserved, the Action writes UPDATED, and the View badge changes from blue to green"
            width="1080"
            height="1440"
            loading="lazy"
            decoding="async"
          />
          <figcaption>
            One committed App transition on ESP32-S3: the existing SQLite value survives, the new
            Action changes subsequent writes and the new View presents the result differently.
            Data, Actions and View advanced together without rebuilding Firmware.
          </figcaption>
        </figure>

        <article className="blog-article-content">
          <section>
            <h2>The development environment is the closed loop</h2>
            <p>
              PocketPi is not a general-purpose development machine. It cannot build an arbitrary
              npm project, provide Node compatibility or let an ordinary App rewrite Firmware and
              native security boundaries.
            </p>
            <p>
              What it can do is more precise: Pi Agent can inspect, modify, validate and commit the
              complete source boundary of an admitted App on the device where that App is running.
              That is possible because earlier architecture decisions line up with the four
              responsibilities:
            </p>
            <ul>
              <li>workspace ownership and App isolation make source inspectable;</li>
              <li>bounded file and lifecycle Tools make a candidate editable;</li>
              <li>App-owned SQLite and forward migrations make current Data testable;</li>
              <li>raw JavaScript, the View SDK and one PocketJS substrate make a coherent version directly executable.</li>
            </ul>
            <p className="article-closing">
              The result is not a small microcontroller pretending to be a Mac. It is an ESP32
              that understands enough about its own App model to close a real software
              development loop on itself.
            </p>
          </section>
        </article>

        <nav className="blog-article-footer-nav" aria-label="Article navigation">
          <a href="/blog">← All articles</a>
          <a href="/docs/app-guide">Read the App contract →</a>
        </nav>
      </main>
      <SiteFooter />
    </div>
  );
}
