import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./product.css";

class ErrorBoundary extends Component<
  { children: ReactNode },
  { error?: Error }
> {
  public override state: { error?: Error } = {};

  public static getDerivedStateFromError(error: Error) {
    return { error };
  }

  public override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Sleepy Doll render failed", error, info);
  }

  public override render() {
    if (this.state.error) {
      return (
        <main className="fatal-error">
          <h1>界面载入失败</h1>
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
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
