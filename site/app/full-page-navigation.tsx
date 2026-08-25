"use client";

import type { MouseEvent, ReactNode } from "react";

export function FullPageNavigation({ children }: { children: ReactNode }) {
  function followInternalLink(event: MouseEvent<HTMLDivElement>) {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    ) return;

    const target = event.target;
    if (!(target instanceof Element)) return;
    const anchor = target.closest<HTMLAnchorElement>("a[href]");
    if (!anchor || anchor.target === "_blank" || anchor.hasAttribute("download")) return;

    const url = new URL(anchor.href, window.location.href);
    if (url.origin !== window.location.origin || url.pathname === window.location.pathname && url.hash) return;

    event.preventDefault();
    event.stopPropagation();
    window.location.assign(url.href);
  }

  return (
    <div data-full-page-navigation onClickCapture={followInternalLink}>
      {children}
    </div>
  );
}
