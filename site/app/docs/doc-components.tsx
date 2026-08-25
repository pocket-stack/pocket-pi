import type { ReactNode } from "react";

export type DocRecord = {
  slug: string;
  title: string;
  description: string;
  render: () => ReactNode;
};

export function Code({ children }: { children: string }) {
  return <pre><code>{children}</code></pre>;
}

export function SourceLink({ path, children }: { path: string; children?: ReactNode }) {
  return (
    <a href={`https://github.com/pocket-stack/pocket-pi/blob/main/${path}`} target="_blank" rel="noreferrer">
      {children ?? path} ↗
    </a>
  );
}

export function DocLead({ children }: { children: ReactNode }) {
  return <p className="doc-intro">{children}</p>;
}

export function PageGoal({ children }: { children: ReactNode }) {
  return (
    <aside className="doc-goal">
      <strong>What this page gives you</strong>
      <div>{children}</div>
    </aside>
  );
}

export function Fact({ children }: { children: ReactNode }) {
  return <blockquote className="doc-fact"><p>{children}</p></blockquote>;
}

export function Status({ children }: { children: ReactNode }) {
  return <span className="doc-status">{children}</span>;
}
