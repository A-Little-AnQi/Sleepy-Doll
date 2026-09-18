import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { SetupApp } from "./SetupApp";
import "../product.css";
import "../motion.css";
import "./setup.css";

/** 安装窗口出问题时也要留下能看的一句话，空白窗口没法排查。 */
class ErrorBoundary extends Component<
  { children: ReactNode },
  { error?: Error }
> {
  public override state: { error?: Error } = {};

  public static getDerivedStateFromError(error: Error) {
    return { error };
  }

  public override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Sleepy Doll setup failed", error, info);
  }

  public override render() {
    if (this.state.error) {
      return (
        <main className="fatal-error">
          <h1>安装界面载入失败</h1>
          <pre>{this.state.error.message}</pre>
        </main>
      );
    }
    return this.props.children;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <SetupApp />
    </ErrorBoundary>
  </StrictMode>,
);
