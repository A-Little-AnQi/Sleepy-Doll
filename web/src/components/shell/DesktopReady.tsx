import { useEffect } from "react";

/** The offscreen native host stays warm; expose it only after fonts and two styled frames. */
export function DesktopReady() {
  useEffect(() => {
    if (!window.ipc) return;
    let cancelled = false;
    let frame = 0;
    void document.fonts.ready.then(() => {
      if (cancelled) return;
      frame = requestAnimationFrame(() => {
        frame = requestAnimationFrame(() => {
          if (!cancelled)
            window.ipc?.postMessage(
              JSON.stringify({
                id: "desktop-ready",
                method: "window.ready",
                params: {
                  dark: document.documentElement.dataset.theme === "dark",
                },
              }),
            );
        });
      });
    });
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
    };
  }, []);
  return null;
}
