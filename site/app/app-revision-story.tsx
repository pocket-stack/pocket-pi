"use client";

import { useState, type CSSProperties } from "react";

type ActiveMoment = "checkout" | "modify" | "review";
type CodeTopic = "data" | "view" | "mount";

const codeTopics: Record<CodeTopic, { title: string; summary: string; detail: string }> = {
  data: {
    title: "Project App data into View state",
    summary: "PocketPi.projection.many reads the App-owned SQLite database and updates the local View model.",
    detail: "The View receives a deliberate projection of durable product data. Rendering code does not own the database and does not duplicate the product state.",
  },
  view: {
    title: "Compose the screen in JavaScript",
    summary: "View.Screen, View.Header and View.Column describe the human-facing interface as composable JavaScript.",
    detail: "The same source file expresses structure, data binding and interaction. The Agent can change that source with ordinary file Tools.",
  },
  mount: {
    title: "Keep the View synchronized",
    summary: "View.mount(render) attaches the composition to the runtime and renders again when projected state changes.",
    detail: "A successful App Action updates durable state. The Projection refreshes the model, then the mounted View reflects the new product state.",
  },
};

const pointerPosition: Record<ActiveMoment, string> = {
  checkout: "30%",
  modify: "50%",
  review: "70%",
};

export function AppRevisionStory() {
  const [activeMoment, setActiveMoment] = useState<ActiveMoment | null>(null);
  const [codeTopic, setCodeTopic] = useState<CodeTopic>("data");

  const activate = (moment: ActiveMoment) => setActiveMoment(moment);
  const stageStyle = activeMoment
    ? ({ "--stage-pointer": pointerPosition[activeMoment] } as CSSProperties)
    : undefined;

  return (
    <figure
      className="iteration-story revision-story"
      aria-label="Five moments in one Agent-led App revision from request through activation"
    >
      <div className="iteration-story-heading">
        <span>One App revision</span>
        <i aria-hidden="true" />
      </div>

      <ol className="revision-track">
        <li className="revision-moment revision-moment-static">
          <div className="revision-node"><span>Request</span></div>
          <h3>Describe the change</h3>
          <p>The human explains what the App should do differently.</p>
        </li>

        <li className={`revision-moment ${activeMoment === "checkout" ? "is-active" : ""}`}>
          <button
            type="button"
            aria-expanded={activeMoment === "checkout"}
            aria-controls="revision-detail-stage"
            onMouseEnter={() => activate("checkout")}
            onFocus={() => activate("checkout")}
            onClick={() => activate("checkout")}
          >
            <span className="revision-node"><span>Checkout</span></span>
            <span className="revision-copy"><strong>Get editable source</strong><small>Pi Agent creates a workspace checkout.</small></span>
          </button>
        </li>

        <li className={`revision-moment ${activeMoment === "modify" ? "is-active" : ""}`}>
          <button
            type="button"
            aria-expanded={activeMoment === "modify"}
            aria-controls="revision-detail-stage"
            onMouseEnter={() => activate("modify")}
            onFocus={() => activate("modify")}
            onClick={() => activate("modify")}
          >
            <span className="revision-node"><span>Modify</span></span>
            <span className="revision-copy"><strong>Change the App</strong><small>File Tools revise Actions and View source.</small></span>
          </button>
        </li>

        <li className={`revision-moment ${activeMoment === "review" ? "is-active" : ""}`}>
          <button
            type="button"
            aria-expanded={activeMoment === "review"}
            aria-controls="revision-detail-stage"
            onMouseEnter={() => activate("review")}
            onFocus={() => activate("review")}
            onClick={() => activate("review")}
          >
            <span className="revision-node"><span>Review</span></span>
            <span className="revision-copy"><strong>Stage the candidate</strong><small>A person decides whether it becomes active.</small></span>
          </button>
        </li>

        <li className="revision-moment revision-moment-static">
          <div className="revision-node"><span>Activate</span></div>
          <h3>Confirm on device</h3>
          <p>The approved release keeps the existing App Data.</p>
        </li>
      </ol>

      {activeMoment ? (
        <div id="revision-detail-stage" className="revision-detail-stage" style={stageStyle}>
          <div key={activeMoment} className="revision-stage-enter">
            {activeMoment === "checkout" ? <CheckoutDetail /> : null}
            {activeMoment === "modify" ? (
              <ModifyDetail topic={codeTopic} onTopicChange={setCodeTopic} />
            ) : null}
            {activeMoment === "review" ? <ReviewDetail /> : null}
          </div>
        </div>
      ) : (
        <p className="revision-hint">Move over Checkout, Modify or Review to inspect that moment.</p>
      )}
    </figure>
  );
}

function CheckoutDetail() {
  return (
    <section className="revision-panel revision-checkout-panel" aria-label="Checkout Tool details">
      <header className="revision-panel-header">
        <div><span>Pi Agent Tool</span><h3>Checkout creates an editable workspace</h3></div>
        <code>app.checkout</code>
      </header>
      <div className="checkout-panel-body">
        <div className="checkout-call">
          <div><span>Tool call</span><code>{'app.checkout({ "id": "exa" })'}</code></div>
          <i aria-hidden="true" />
          <div><span>Returned path</span><code>apps/exa/checkout</code></div>
        </div>
        <div className="checkout-meaning">
          <article>
            <h4>What the Tool does</h4>
            <p>It asks AppSupervisor to copy the installed App source into Pi Agent&apos;s workspace. Later calls reopen the same work.</p>
          </article>
          <article>
            <h4>What the path means</h4>
            <p><code>apps/exa/checkout</code> is the editable checkout root. The Agent reads and changes files such as <code>app.json</code>, <code>actions.js</code> and <code>view.js</code> inside it.</p>
          </article>
          <article>
            <h4>What is not copied</h4>
            <p>Live App Data, temporary files and credentials remain under runtime ownership. Editing the checkout cannot mutate the installed release.</p>
          </article>
        </div>
      </div>
    </section>
  );
}

function ModifyDetail({ topic, onTopicChange }: { topic: CodeTopic; onTopicChange: (topic: CodeTopic) => void }) {
  const explanation = codeTopics[topic];

  return (
    <section className="revision-panel revision-modify-panel" aria-label="Modify the App with File Tools">
      <header className="revision-panel-header">
        <div><span>File Tools</span><h3>The Agent composes the App it lives on</h3></div>
        <code>apps/exa/view.js</code>
      </header>
      <div className="modify-inspector">
        <aside className="modify-explanation" aria-live="polite">
          <span>Selected code</span>
          <h4>{explanation.title}</h4>
          <p>{explanation.summary}</p>
          <p>{explanation.detail}</p>
          <small>Select another code block to inspect its responsibility.</small>
        </aside>
        <div className="modify-source" aria-label="Interactive excerpts from the current Exa View">
          <button type="button" className={topic === "data" ? "is-selected" : ""} aria-pressed={topic === "data"} onClick={() => onTopicChange("data")}>
            <code>{`const historyProjection = PocketPi.projection.many(
  \`SELECT id, query, status, result_count
   FROM searches ORDER BY id DESC\`,
  params,
  rows => model.update({ history: rows })
);`}</code>
          </button>
          <button type="button" className={topic === "view" ? "is-selected" : ""} aria-pressed={topic === "view"} onClick={() => onTopicChange("view")}>
            <code>{`return View.Screen({ children: [
  View.Header({ title: "EXA RESEARCH" }),
  View.Column({ children: state.history.map(historyCard) })
] });`}</code>
          </button>
          <button type="button" className={topic === "mount" ? "is-selected" : ""} aria-pressed={topic === "mount"} onClick={() => onTopicChange("mount")}>
            <code>View.mount(render);</code>
          </button>
        </div>
      </div>
    </section>
  );
}

function ReviewDetail() {
  return (
    <section className="revision-panel revision-review-panel" aria-label="Human approval for a staged App candidate">
      <header className="revision-panel-header">
        <div><span>Human approval</span><h3>The candidate is staged, not installed</h3></div>
        <code>app.submit</code>
      </header>
      <div className="review-panel-body">
        <div className="review-meaning">
          <article><h4>Before approval</h4><p>The installed source and App Data remain active. Validation alone does not replace the product.</p></article>
          <article><h4>What the human reviews</h4><p>The device shows the candidate identity, version, Tools, schedules, capabilities and credential declarations.</p></article>
          <article><h4>After INSTALL</h4><p>The staged source becomes active while the existing App-owned Data remains in place.</p></article>
        </div>
        <div className="approval-preview" aria-label="Preview of the physical Human Approval screen">
          <div className="approval-preview-header"><span>Review App update</span><b>Candidate staged</b></div>
          <div className="approval-app"><span>EXA</span><div><strong>Exa Research</strong><small>Installed 1.1.0</small></div></div>
          <dl className="approval-facts">
            <div><dt>Tools</dt><dd>2</dd></div>
            <div><dt>Schedules</dt><dd>0</dd></div>
            <div><dt>Capability</dt><dd>net.http</dd></div>
            <div><dt>Credential</dt><dd>exa.api-key</dd></div>
          </dl>
          <p>Candidate validation passed. Installation still requires a person on the product.</p>
          <div className="approval-actions"><button type="button" disabled>DISMISS</button><button type="button" disabled>INSTALL</button></div>
        </div>
      </div>
    </section>
  );
}
