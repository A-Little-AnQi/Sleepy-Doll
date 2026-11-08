import type { Bootstrap, TaskSummary } from "../../ipc/types";
import { SchedulesPanel } from "./SchedulesPanel";
import "./SchedulesPage.css";

export function SchedulesPage({
  bootstrap,
  reload,
  initialTask,
  onConsumed,
}: {
  bootstrap: Bootstrap;
  reload(): Promise<void>;
  initialTask?: TaskSummary | undefined;
  onConsumed(): void;
}) {
  return (
    <div className="page-sheet schedules-page">
      <SchedulesPanel
        bootstrap={bootstrap}
        reload={reload}
        initialTask={initialTask}
        onConsumed={onConsumed}
      />
    </div>
  );
}
