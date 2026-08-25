export const githubUrl = "https://github.com/pocket-stack/pocket-pi";

export type Section = "home" | "docs" | "blog" | "changelog";

function Mark() {
  // The tiny local SVG is already the final optimized brand asset.
  // eslint-disable-next-line @next/next/no-img-element
  return <img className="brand-mark" src="/favicon.svg" alt="" aria-hidden="true" />;
}

function GitHubIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 .5C5.7.5.5 5.7.5 12c0 5.1 3.3 9.4 7.9 10.9.6.1.8-.3.8-.6v-2c-3.2.7-3.9-1.5-3.9-1.5-.5-1.3-1.3-1.7-1.3-1.7-1.1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1 1.8 2.7 1.3 3.4 1 .1-.8.4-1.3.8-1.6-2.6-.3-5.3-1.3-5.3-5.8 0-1.3.5-2.3 1.2-3.1-.1-.3-.5-1.5.1-3.1 0 0 1-.3 3.3 1.2a11.5 11.5 0 0 1 6 0C17 4.7 18 5 18 5c.6 1.6.2 2.8.1 3.1.8.8 1.2 1.8 1.2 3.1 0 4.5-2.7 5.5-5.3 5.8.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6 4.6-1.5 7.9-5.8 7.9-10.9C23.5 5.7 18.3.5 12 .5z" />
    </svg>
  );
}

export function SiteHeader({ active = "home" }: { active?: Section }) {
  return (
    <header className="topbar">
      <div className="topbar-inner">
        <a className="brand" href="/" aria-label="PocketPi home">
          <Mark />
          <span>PocketPi</span>
        </a>
        <nav className="nav-links" aria-label="Primary navigation">
          <a className={active === "docs" ? "active" : ""} href="/docs">Docs</a>
          <a className={active === "blog" ? "active" : ""} href="/blog">Blog</a>
          <a className={active === "changelog" ? "active" : ""} href="/changelog">Changelog</a>
          <a className="nav-github" href={githubUrl} target="_blank" rel="noreferrer" aria-label="PocketPi on GitHub">
            <GitHubIcon />
          </a>
        </nav>
      </div>
    </header>
  );
}

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="container footer-grid">
        <div className="footer-about">
          <a className="brand" href="/">
            <Mark />
            <span>PocketPi</span>
          </a>
          <p>An Agent-native runtime for embedded and dedicated devices, built on PocketJS.</p>
        </div>
        <div>
          <h4>Docs</h4>
          <a href="/docs">Overview</a>
          <a href="/docs/mental-model">Mental model</a>
          <a href="/docs/app-quickstart">Build an App</a>
          <a href="/docs/security">Security</a>
        </div>
        <div>
          <h4>Project</h4>
          <a href="/blog">Blog</a>
          <a href="/changelog">Changelog</a>
          <a href={githubUrl} target="_blank" rel="noreferrer">GitHub ↗</a>
          <a href="https://pocketlab.build/" target="_blank" rel="noreferrer">Pocket Lab ↗</a>
        </div>
      </div>
      <div className="container footer-bottom">
        <span>© 2026 Pocket Lab</span>
      </div>
    </footer>
  );
}
