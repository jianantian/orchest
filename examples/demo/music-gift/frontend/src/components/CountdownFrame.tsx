import { useEffect, useRef, useState } from "react";

/**
 * Renders the LLM-generated countdown scene inside a sandboxed iframe.
 *
 * The scene is not trusted: it is model output built from user-supplied
 * name/scenario text, so it is a prompt-injection → XSS vector. But it also
 * legitimately needs to run a `setInterval` countdown, so we can't just strip
 * scripts. `sandbox="allow-scripts"` (deliberately WITHOUT allow-same-origin)
 * lets the timer run while giving the frame an opaque origin with no access to
 * this page's cookies, localStorage, session, or DOM.
 *
 * Height: cross-origin frames can't be measured from the parent, so we inject
 * our own trusted reporter that posts scrollHeight. The untrusted content could
 * post a forged height too, so the value is clamped.
 */

const MAX_HEIGHT = 2000;

const HEIGHT_REPORTER = `
<script>
  (function () {
    function report() {
      var h = document.documentElement.scrollHeight;
      parent.postMessage({ __moment_cd_height: h }, "*");
    }
    window.addEventListener("load", report);
    new ResizeObserver(report).observe(document.documentElement);
    setTimeout(report, 100);
  })();
<\/script>`;

const FRAME_STYLE = `
<style>
  html, body { margin: 0; padding: 0; background: transparent; }
</style>`;

export function CountdownFrame({ html }: { html: string }) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(0);

  useEffect(() => {
    function onMessage(e: MessageEvent) {
      // Only trust messages from this iframe's own window. The sandboxed frame
      // has an opaque origin ("null"), so we match on source, not origin.
      if (e.source !== ref.current?.contentWindow) return;
      const h = (e.data as { __moment_cd_height?: unknown })?.__moment_cd_height;
      if (typeof h === "number" && h > 0) {
        setHeight(Math.min(Math.ceil(h), MAX_HEIGHT));
      }
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  const srcDoc = `${FRAME_STYLE}${html}${HEIGHT_REPORTER}`;

  return (
    <iframe
      ref={ref}
      className="countdown-section"
      title="Countdown"
      sandbox="allow-scripts"
      srcDoc={srcDoc}
      style={{ height: height ? `${height}px` : "260px", border: "none", width: "100%" }}
    />
  );
}
