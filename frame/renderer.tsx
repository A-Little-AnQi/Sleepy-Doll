import type { CSSProperties, ReactNode } from "react";
import type { LibraryRenderer, Props, RenderProps, Theme } from "loom-frame";
import {
  Badge,
  Button,
  Checkbox,
  ComposerDeck,
  ComposerField,
  ConfirmDialog,
  Dialog,
  DisclosureChevron,
  EmptyState,
  ExtensionList,
  ExtensionRow,
  FormField,
  IconButton,
  InlineRename,
  MotionSwitch,
  SearchField,
  Select,
  SettingRow,
  SettingsScope,
  SlidingTabs,
  Switch,
  TaskCard,
  TextField,
  ThemeSwitch,
  Toast,
  type TaskActions,
  type TaskCardData,
} from "@sleepy-doll/ui";
import * as Icons from "@sleepy-doll/ui/icons";
import "@sleepy-doll/ui/styles.css";
import "@sleepy-doll/ui/motion.css";
import "./preview.css";

const str = (p: Props, key: string) => String(p[key] ?? "");
const checked = (p: Props, key: string) => Boolean(p[key]);
const width: CSSProperties = { width: "min(520px, 100%)", minWidth: 0 };
const inside = (children: ReactNode) => <div style={width}>{children}</div>;
// 正式页面使用这些入场容器；保留它们的动效和文字合成上下文。
const animated = (children: ReactNode, kind: "page" | "panel" = "page") => (
  <MotionSwitch viewKey="preview" kind={kind}>
    {children}
  </MotionSwitch>
);
const buttonVariant = (value: string) =>
  value === "primary" || value === "runtime" || value === "subtle"
    ? value
    : "secondary";

function render({ entry, props: p, onPatch }: RenderProps): ReactNode {
  const id = entry.id.slice("sleepy-doll:".length);
  const change = (key: string) => (value: string | boolean) =>
    onPatch(key, value);
  const field = (composer = false) =>
    composer ? (
      <ComposerField
        value={str(p, "value")}
        placeholder={str(p, "placeholder")}
        disabled={checked(p, "disabled")}
        onChange={(e) => onPatch("value", e.target.value)}
      />
    ) : (
      <TextField
        aria-label="预览输入"
        value={str(p, "value")}
        placeholder={str(p, "placeholder")}
        disabled={checked(p, "disabled")}
        readOnly={checked(p, "readOnly")}
        type={str(p, "type") || "text"}
        onChange={(e) => onPatch("value", e.target.value)}
      />
    );
  switch (id) {
    case "button":
      return (
        <Button
          variant={buttonVariant(str(p, "variant"))}
          disabled={checked(p, "disabled")}
          onClick={() =>
            onPatch(
              "children",
              str(p, "children") === "已点击" ? "继续" : "已点击",
            )
          }
        >
          {str(p, "children")}
        </Button>
      );
    case "icon-button":
      return (
        <IconButton
          type="button"
          aria-label={str(p, "label")}
          title={str(p, "label")}
          disabled={checked(p, "disabled")}
          onClick={() => onPatch("clicks", Number(p.clicks ?? 0) + 1)}
        >
          <Icons.PlusIcon className="button-icon" />
        </IconButton>
      );
    case "badge":
      return <Badge active={checked(p, "active")}>{str(p, "children")}</Badge>;
    case "switch":
      return (
        <Switch
          checked={checked(p, "checked")}
          label={str(p, "label")}
          disabled={checked(p, "disabled")}
          onChange={change("checked")}
        />
      );
    case "checkbox":
      return (
        <Checkbox
          checked={checked(p, "checked")}
          label={str(p, "label")}
          disabled={checked(p, "disabled")}
          onChange={change("checked")}
        />
      );
    case "text-field":
      return inside(field());
    case "textarea":
      return inside(
        <TextField
          multiline
          aria-label="配置内容"
          value={str(p, "value")}
          placeholder={str(p, "placeholder")}
          disabled={checked(p, "disabled")}
          onChange={(e) => onPatch("value", e.target.value)}
        />,
      );
    case "select": {
      const options = Array.isArray(p.options)
        ? p.options.map((value) => {
            const item =
              value && typeof value === "object" && !Array.isArray(value)
                ? value
                : {};
            return {
              value: String(item.value ?? ""),
              label: String(item.label ?? ""),
              ...(item.description
                ? { description: String(item.description) }
                : {}),
            };
          })
        : [];
      return inside(
        <Select
          value={str(p, "value")}
          label={str(p, "label")}
          placeholder={str(p, "placeholder")}
          options={options}
          disabled={checked(p, "disabled")}
          onChange={change("value")}
        />,
      );
    }
    case "search-field":
      return inside(
        <SearchField
          aria-label="搜索组件"
          value={str(p, "value")}
          placeholder={str(p, "placeholder")}
          compact={checked(p, "compact")}
          disabled={checked(p, "disabled")}
          onChange={(e) => onPatch("value", e.target.value)}
        />,
      );
    case "inline-rename":
      return inside(
        <InlineRename
          value={str(p, "value")}
          label={str(p, "label")}
          onChange={change("value")}
          onSubmit={() => onPatch("outcome", "已提交")}
          onCancel={() => onPatch("outcome", "已取消")}
        />,
      );
    case "sliding-tabs": {
      const items = Array.isArray(p.items)
        ? p.items.map((value) => {
            const item =
              value && typeof value === "object" && !Array.isArray(value)
                ? value
                : {};
            return { id: String(item.id ?? ""), name: String(item.name ?? "") };
          })
        : [];
      return inside(
        <SlidingTabs
          ariaLabel="设置分类"
          value={str(p, "value")}
          items={items}
          onChange={change("value")}
        />,
      );
    }
    case "theme-switch":
      return (
        <SettingsScope style={{ width: 140 }}>
          {animated(
            <ThemeSwitch
              theme={p.theme === "dark" ? "dark" : "light"}
              onChange={(value) => onPatch("theme", value)}
            />,
          )}
        </SettingsScope>
      );
    case "settings-scope":
      return inside(
        <SettingsScope>
          <SettingRow label="主题">
            <ThemeSwitch
              theme={p.theme === "dark" ? "dark" : "light"}
              onChange={(value) => onPatch("theme", value)}
            />
          </SettingRow>
          <SettingRow label="启用扩展">
            <Switch
              label="启用扩展"
              checked={checked(p, "checked")}
              onChange={change("checked")}
            />
          </SettingRow>
        </SettingsScope>,
      );
    case "disclosure-chevron":
      return <DisclosureChevron expanded={checked(p, "expanded")} />;
    case "form-field":
      return inside(
        <div className="form-section">
          <FormField label={str(p, "label")}>{field()}</FormField>
        </div>,
      );
    case "setting-row":
      return inside(
        <SettingRow
          label={str(p, "label")}
          hint={str(p, "hint")}
          compact={checked(p, "compact")}
        >
          <Switch
            label={str(p, "label")}
            checked={checked(p, "checked")}
            onChange={change("checked")}
          />
        </SettingRow>,
      );
    case "empty-state":
      return inside(
        <EmptyState
          icon={<Icons.HistoryIcon />}
          title={str(p, "title")}
          description={str(p, "description")}
        >
          {p.action && (
            <Button
              onClick={() => onPatch("clicks", Number(p.clicks ?? 0) + 1)}
            >
              {str(p, "action")}
            </Button>
          )}
        </EmptyState>,
      );
    case "dialog":
      return (
        <>
          <Button onClick={() => onPatch("open", true)}>打开弹窗</Button>
          <Dialog
            open={checked(p, "open")}
            compact={checked(p, "compact")}
            title={str(p, "title")}
            subtitle={str(p, "subtitle")}
            onClose={() => onPatch("open", false)}
            footer={
              <>
                <Button variant="subtle" onClick={() => onPatch("open", false)}>
                  取消
                </Button>
                <Button
                  variant="primary"
                  onClick={() => onPatch("open", false)}
                >
                  保存
                </Button>
              </>
            }
          >
            <p>{str(p, "content")}</p>
          </Dialog>
        </>
      );
    case "confirm-dialog":
      return (
        <>
          <Button onClick={() => onPatch("open", true)}>打开确认弹窗</Button>
          <ConfirmDialog
            open={checked(p, "open")}
            title={str(p, "title")}
            confirmLabel={str(p, "confirmLabel")}
            busy={checked(p, "busy")}
            onClose={() => onPatch("open", false)}
            onConfirm={() => {
              onPatch("confirmed", true);
              onPatch("open", false);
            }}
          >
            <p>{str(p, "content")}</p>
          </ConfirmDialog>
        </>
      );
    case "toast":
      return (
        <>
          <Button onClick={() => onPatch("visible", true)}>显示提示</Button>
          {checked(p, "visible") && (
            <Toast
              message={str(p, "message")}
              duration={Number(p.duration) || 0}
              onDismiss={() => onPatch("visible", false)}
              {...(p.action
                ? {
                    action: {
                      label: str(p, "action"),
                      onAction: () => onPatch("clicked", true),
                    },
                  }
                : {})}
            />
          )}
        </>
      );
    case "extension-row":
      return inside(
        animated(
          <ExtensionList>
            <ExtensionRow
              name={str(p, "name")}
              meta={str(p, "meta")}
              description={str(p, "description")}
              flag={str(p, "flag")}
              enabled={checked(p, "enabled")}
              disabled={checked(p, "disabled")}
              onOpen={() => onPatch("opened", true)}
              onChange={change("enabled")}
            />
          </ExtensionList>,
          "panel",
        ),
      );
    case "extension-list":
      return inside(
        <ExtensionList>
          <ExtensionRow
            name="create-shortcut"
            meta="内置"
            description="将已有配置保存为快捷任务。"
            enabled={checked(p, "enabled")}
            onOpen={() => onPatch("enabled", !checked(p, "enabled"))}
            onChange={change("enabled")}
          />
          <ExtensionRow
            name="route-helper"
            meta="本机"
            description="整理路线与任务配置。"
            enabled={false}
            onOpen={() => onPatch("enabled", false)}
            onChange={change("enabled")}
          />
        </ExtensionList>,
      );
    case "task-card": {
      const task: TaskCardData = {
        id: "preview",
        name: str(p, "name"),
        description: str(p, "description"),
        pinned: checked(p, "pinned"),
        state: str(p, "state"),
        stateLabel:
          p.state === "archived"
            ? "已归档"
            : p.state === "unavailable"
              ? "暂不可用"
              : "可运行 · 尚未实机验证",
        actionLabel:
          p.state === "archived"
            ? "恢复"
            : p.state === "unavailable"
              ? "连接应用"
              : "运行",
        runnable: p.state === "readyUnverified",
        zeroToken: true,
        sourceDeleted: false,
        issue: str(p, "issue"),
        shortcut: { applicationName: "BetterGI", targetName: "采集路线组" },
      };
      const actions: TaskActions = {
        run: () => {
          onPatch("running", true);
          onPatch("lastAction", "运行");
        },
        stop: () => {
          onPatch("running", false);
          onPatch("lastAction", "停止");
        },
        pin: (_, value) => onPatch("pinned", value),
        archive: (_, value) =>
          onPatch("state", value ? "archived" : "readyUnverified"),
        rename: () => onPatch("lastAction", "重命名"),
        remove: () => onPatch("lastAction", "删除"),
        connect: () => onPatch("lastAction", "连接应用"),
        open: () => onPatch("lastAction", "打开"),
      };
      return inside(
        animated(
          <TaskCard
            task={task}
            actions={actions}
            busy={checked(p, "busy")}
            showDescription={checked(p, "showDescription")}
            running={checked(p, "running")}
            activeRun={
              checked(p, "running") ? { state: "executing" } : undefined
            }
            runStateLabel="正在执行"
          />,
        ),
      );
    }
    case "composer-field":
      return inside(field(true));
    case "composer-deck":
      return inside(
        <ComposerDeck>
          {field(true)}
          <div className="frame-composer-actions">
            <Button
              variant="subtle"
              onClick={() => onPatch("value", "创建快捷任务")}
            >
              创建快捷任务
            </Button>
            <Button
              variant="primary"
              disabled={!str(p, "value").trim()}
              onClick={() => onPatch("submitted", true)}
            >
              发送
            </Button>
          </div>
        </ComposerDeck>,
      );
    case "motion-switch":
      return inside(
        <MotionSwitch
          viewKey={str(p, "viewKey")}
          kind={p.kind === "page" ? "page" : "panel"}
        >
          <div className="notice">{str(p, "content")}</div>
        </MotionSwitch>,
      );
    case "brand-icon":
      return <Icons.BrandIcon />;
    case "wordmark":
      return <Icons.Wordmark />;
    default: {
      const Icon = Icons[entry.name as keyof typeof Icons];
      if (Icon) return <Icon className="button-icon" />;
      throw new Error("尚未接入组件：" + entry.id);
    }
  }
}

let previous: string[] = [];
function applyTheme(theme: Theme, tokens: Record<string, string>) {
  const root = document.documentElement;
  for (const name of previous) root.style.removeProperty(name);
  root.dataset.theme = theme.mode;
  // 不同语义颜色分别保留；强调色只对应品牌金色。
  const values: Record<string, string> = {
    "--surface": theme.surface,
    "--text": theme.text,
    "--muted": theme.muted,
    "--border": theme.border,
    "--gold": theme.accent,
    "--radius": `${theme.radius}px`,
    "--radius-sm": `${theme.radius * 0.75}px`,
    "--layout-radius": `${theme.radius * 2}px`,
    "--font-body": `${theme.fontSize}px`,
    "--motion-fast": `${theme.motion / 2}ms`,
    "--motion-base": `${theme.motion}ms`,
    "--motion-slow": `${(theme.motion * 4) / 3}ms`,
    "--border-width": `${theme.borderWidth}px`,
    ...(theme.shadow === "none" ? { "--shadow": "none" } : {}),
    ...tokens,
  };
  for (const [name, value] of Object.entries(values))
    root.style.setProperty(name, value);
  previous = Object.keys(values);
}

export const renderer: LibraryRenderer = {
  render,
  applyTheme,
  renderScene: (props) => (
    <div className="frame-scene">
      <header>
        <Icons.Wordmark />
        <h1>{props.entry.label}</h1>
        <p>与主程序共用 @sleepy-doll/ui；以下操作仅修改预览参数。</p>
      </header>
      <section>{render(props)}</section>
    </div>
  ),
};
