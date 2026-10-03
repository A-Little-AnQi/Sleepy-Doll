import { ease, lerp, seg } from "./clock";
import { cueTime } from "./sync";

type Bounds = { x: number; y: number; w: number; h: number; naturalWidth?: number };
type Camera = { scale: number; tx: number; ty: number };

// 产品对象的接续、审批停顿和入口收拢共用实际 DOM 坐标。
export function ProductRelay({ time, story, bounds, camera, taskHtml, previewHtml }: {
  time: number; story: number; bounds: Record<string, Bounds>; camera: Camera;
  planHtml?: string; taskHtml?: string; previewHtml?: string;
}) {
  const screen = (b: Bounds) => ({
    x: b.x * camera.scale + camera.tx, y: b.y * camera.scale + camera.ty,
    w: b.w * camera.scale, h: b.h * camera.scale,
  });
  const host = cueTime("host"), save = cueTime("save"), saved = cueTime("saved-task");
  const linking = time >= host - 280 && time < host + 1150;
  const linkFrom = bounds.step1 ?? bounds.plan;
  const linkTo = bounds.bgiRow;
  const transfer = time >= save && time < saved + 520;
  const source = bounds.futurePreview, destination = bounds.futureTask;
  const k = ease.transition(seg(time, save + 90, Math.max(save + 650, saved + 200)));
  const approval = bounds.approval;
  const confirm = time >= cueTime("approval") && time < cueTime("allow") + 120;
  const activeStep = ["plan-check", "plan-route", "plan-verify"].findIndex((id, i, ids) =>
    time >= cueTime(id) && (i === 2 ? story < 9000 : time < cueTime(ids[i + 1]!)));
  const step = bounds[`step${activeStep}`];
  return (
    <div className="product-relay" aria-hidden>
      {step && activeStep >= 0 && (() => {
        const b = screen(step), at = cueTime(["plan-check", "plan-route", "plan-verify"][activeStep]!);
        const enter = ease.camera(seg(time, at, at + 230));
        return <div className="relay-step" style={{ left: b.x - b.w / 2 - 14, top: b.y - b.h / 2 - 5,
          width: b.w + 28, height: b.h + 10, opacity: enter, clipPath: `inset(0 ${(1 - enter) * 100}% 0 0)` }} />;
      })()}
      {confirm && approval && (() => {
        const b = screen(approval), k = ease.camera(seg(time, cueTime("approval"), cueTime("approval") + 240));
        return <div className="relay-confirm" style={{ left: b.x - b.w / 2 - 16, top: b.y - b.h / 2 - 16,
          width: b.w + 32, height: b.h + 32, opacity: k }}><span>确认后执行</span></div>;
      })()}
      {linking && linkFrom && linkTo && (() => {
        const a = screen(linkFrom), b = screen(linkTo), q = ease.camera(seg(time, host - 180, host + 600));
        const x0 = a.x + a.w / 2, y0 = a.y, x1 = b.x - b.w / 2, y1 = b.y;
        const mid = (x0 + x1) / 2;
        return <svg className="relay-link" viewBox="0 0 1920 1080" style={{ opacity: 1 - seg(time, host + 850, host + 1150) }}>
          <path d={`M${x0},${y0} C${mid},${y0} ${mid},${y1} ${x1},${y1}`} pathLength="1"
            strokeDasharray="1" strokeDashoffset={1 - q} />
          <circle cx={lerp(x0, x1, q)} cy={lerp(y0, y1, q)} r="6" />
        </svg>;
      })()}
      {transfer && source && destination && previewHtml && taskHtml && (() => {
        const a = screen(source), b = screen(destination);
        const x = lerp(a.x, b.x, k), y = lerp(a.y, b.y, k) - Math.sin(k * Math.PI) * 90;
        const w = lerp(a.w, b.w, k), h = lerp(a.h, b.h, k);
        return <div className="relay-archive" style={{ left: x - w / 2, top: y - h / 2, width: w, height: h,
          opacity: 1 - seg(time, saved + 300, saved + 520), transform: `rotate(${Math.sin(k * Math.PI) * -3}deg)` }}>
          <div className="relay-copy relay-preview-copy" style={{ width: source.naturalWidth ?? source.w, opacity: 1 - seg(k, 0.38, 0.68),
            transform: `scale(${w / (source.naturalWidth ?? source.w)})` }} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          <div className="relay-copy" style={{ width: destination.naturalWidth ?? destination.w, opacity: seg(k, 0.4, 0.75),
            transform: `scale(${w / (destination.naturalWidth ?? destination.w)})` }} dangerouslySetInnerHTML={{ __html: taskHtml }} />
        </div>;
      })()}
    </div>
  );
}
