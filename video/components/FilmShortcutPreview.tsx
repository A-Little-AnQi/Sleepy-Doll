import { CloseIcon } from "../../web/src/components/icons";
import "../../web/src/components/overlay/Dialog.css";
import "./film-shortcut-preview.css";
import fixture from "../fixtures/launch-film.json";
import { ease, seg } from "../clock";

// 逐帧状态适配：沿用当前快捷入口预览的 DOM 和样式，不调用桌面 IPC。
export function FilmShortcutPreview({ time }: { time: number }) {
  const k = ease.camera(seg(time, 22300, 22700));
  return (
    <div className="film-shortcut" style={{ opacity: time < 23400 ? k : 0 }}>
      <section className="sd-dialog shortcut-configuration-dialog" role="dialog"
        style={{ transform: `translateY(${(1 - k) * 50}px)` }}>
        <header className="sd-dialog-head">
          <div><h2>添加快捷任务</h2><p className="sd-dialog-path">从已有任务或配置中识别所选项目，确认后保存入口。</p></div>
          <button className="icon-button" aria-label="关闭"><CloseIcon className="button-icon" /></button>
        </header>
        <div className="sd-dialog-body">
          <label className="shortcut-form-field"><span>要加入哪一项？</span>
            <textarea readOnly value="把 BetterGI 中已有的每日委托路线加入快捷任务" rows={2} />
          </label>
          <section className="shortcut-preview">
            <div className="shortcut-preview-heading"><h3>入口预览</h3><span>尚未保存</span></div>
            <dl><dt>应用</dt><dd>BetterGI</dd><dt>已有项</dt><dd>每日委托路线</dd></dl>
            <label className="shortcut-form-field"><span>入口名称</span><input readOnly value={fixture.workflow.name} /></label>
            <label className="shortcut-form-field"><span>用途说明</span><textarea readOnly rows={2} value="运行已有的每日委托路线" /></label>
          </section>
        </div>
        <footer className="sd-dialog-foot">
          <span className="shortcut-config-state">确认入口信息后保存即可使用。</span>
          <button className="subtle-action">取消</button>
          <button className="primary-action" data-video-anchor="sd-save">保存入口</button>
        </footer>
      </section>
    </div>
  );
}
