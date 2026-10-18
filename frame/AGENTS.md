# 用 AI 将项目组件库接入 Loom

这份指南给正在操作接入项目的代码 Agent 使用。你的任务是理解该项目、找到真实可复用组件、整理预览适配，并让用户能通过 `loom` 浏览和操作这些组件。Loom-Frame 是框架，`loom-frame` 是 npm 包，`loom` 是命令；框架默认没有登记被预览的组件库。框架界面使用独立的 Loom UI；接入方的组件库仍由配置显式登记。

本文件说明接入方法，不替代接入项目自己的开发规范，也不要求该项目采用 Loom 仓库的目录、Git 流程或工具。

## 1. 先理解用户的项目

在接入项目根目录开始，先读现有 README、AGENTS.md、包清单、工作区配置与启动脚本。确认用户要接入哪套组件库、哪些平台，已有运行方式是什么。已有 Loom 配置时在原配置中增量接入。

沿真实导入关系定位以下内容，而不是只搜索名为 components 的目录：

| 要找的内容 | 从哪里开始 | 用来决定什么 |
| --- | --- | --- |
| 组件公开入口与实现 | package exports、barrel 导出、共享目录、页面内局部组件 | 真实导入路径与可提取范围 |
| 实际调用位置 | 页面、路由、现有 demo / stories、组件调用方 | 默认参数、插槽、常见变体和交互 |
| 样式与资源 | CSS 入口、主题 Provider、Token、字体、图标、图片路径 | 预览需要加载的依赖与主题映射 |
| 运行依赖 | 路由、Context、状态库、国际化、网络及权限接口 | 最小预览容器与需要隔离的业务流程 |
| 工程入口 | React / Vite 配置、路径别名、包管理器、平台构建配置 | 独立启动还是嵌入现有开发服务 |

用项目现有的启动方式理解产品页面。不要凭组件名称猜接口；先读实现，再看调用方。优先复用已有示例数据与 Provider。静态类型、成功启动和实际交互是不同证据，汇报时分别说明。

当前 Web 渲染协议基于 React。其他 Web 技术栈需要能在 React 容器中挂载和卸载真实组件的桥接层；框架不自动转换 Vue、原生 Android 或任意页面。

## 2. 找出可复用组件，整理命名和分类

用户要求提取项目组件时，覆盖共享组件、页面内重复结构和可以独立呈现的组合组件。不要只登记按钮和输入框，也要识别导航入口、信息卡、资料卡、操作组、设置行、消息结构、弹层和状态展示。

每个候选组件记录：实现文件、导出名、调用位置、参数、回调、插槽、样式依赖、Provider，以及实际使用到的基础组件。这个记录服务于接入，不必为了记录另建一套文档目录。

根据可复用的外观与职责命名预览条目。例如，业务中的“提交申请”按钮可呈现为对应的图标按钮、描边按钮或操作卡，而不是沿用业务动作作为通用组件名；个人资料卡本身可以是通用组合组件。只有源码能独立支持该形态时才提取，不把一个业务页面改名后当作通用组件。

`name` 是通用组件名，`label` 是自然中文名。原源码名称可放在 `sourceName` / `aliases` 中，不必为了预览名称重命名业务源码。ID 使用稳定的 `库key:组件名`，例如 `kit:profile-card`，不随显示名称、版本或配置变化。

按当前条目的功能分类：

| 字段 | 用法 |
| --- | --- |
| `kind: 'basic'` | 独立基础控件或视觉原语，例如按钮、输入框、图标、分隔线 |
| `kind: 'composite'` | 有明确结构、组合关系或协调行为的组件，例如资料卡、设置行、操作面板 |
| `group` | 操作、表单、图标、导航、浮层、展示、反馈、布局、设置、语音、消息、资料 |

分段切换器用于切换内容页时属于导航；只有实际作为设置值输入的条目才归表单。不要根据组件文件名中的 Field、Control 等词机械分类。

为组合组件填写真实的 `basicComponentIds`，让介绍下面出现可点击的基础组件链接。先沿导入和嵌套关系找到基础组件并登记，再引用它们的完整 ID；不要按外观猜依赖。`related` 表示相关条目，不代替基础依赖关系。

同步已有清单时保留稳定 ID。需要改 ID 时用 `legacyIds` 关联旧 ID。来源已删除的组件，只有在本适配仍有合法、完整、可运行的保留实现时才标记 `sourceStatus: 'retained'`；没有实现的条目不能伪装成可用预览。按用户意图决定移除或补充保留实现，并说明缺失情况。

## 3. 安装并准备接入文件

使用项目自己的包管理器。下面以用户已提供的本地安装包为例：

```sh
npm install -D ./loom-frame-0.1.0.tgz
npx loom init
```

安装包路径按实际交付位置替换；不要假设 npm 上已经发布了同名包。`loom init` 只创建缺失的 `frame.config.ts` 和 `frame/AGENTS.md`，不会扫描或自动提取组件，也不会覆盖已有文件。`npx loom agents` 可输出随包交付的这份指南。

默认接入结构：

```text
项目根目录/
  frame.config.ts          # 库登记和启动配置
  frame/
    AGENTS.md              # 本接入指南
    catalog.ts             # 组件清单
    renderer.tsx           # 真实组件渲染与交互适配
```

这些路径可通过配置调整。将适配放在用户指定的接入项目或独立适配包，直接导入真实组件；不要把整份项目复制到框架里。如果组件来源是另一个只读项目，在已授权的适配位置完成工作，不向来源项目写生成文件。用户明确要求提取源码成为独立组件库时，再按其授权拆出实现和依赖。

## 4. 登记一套 Web 组件库

```ts
// frame.config.ts
import type {FrameConfig} from 'loom-frame';

export default {
  name: '项目组件预览',
  storage: {namespace: 'project-kit'},
  libraries: [{
    key: 'kit',
    name: 'Project Kit',
    catalog: './frame/catalog.ts',
    renderer: './frame/renderer.tsx',
    sourceRoots: ['src/components'],
    preview: {language: 'tsx'},
  }],
  plugins: [],
} satisfies FrameConfig;
```

库 key 使用小写字母起头，后续可含数字和连字符，不能与其他库重复。清单和渲染器路径相对接入项目根目录。`sourceRoots`、`packages`、`styles` 是源码与用法元信息，不会自动导入源码、样式或安装依赖。

下面假设来源项目已经存在 Button 和 Checkbox；用你读到的真实路径、导出方式和接口替换示例，不要为了套用示例制造假组件。

```ts
// frame/catalog.ts
import type {Entry} from 'loom-frame';

export const catalog: Entry[] = [
  {
    id: 'kit:button', name: 'Button', label: '按钮',
    kind: 'basic', group: '操作', description: '触发调用方提供的操作。',
    package: '', source: 'src/components/Button.tsx',
    props: {children: '继续', disabled: false},
    fields: [
      {key: 'children', label: '文字', kind: 'text'},
      {key: 'disabled', label: '禁用', kind: 'boolean'},
    ],
  },
  {
    id: 'kit:checkbox', name: 'Checkbox', label: '复选框',
    kind: 'basic', group: '表单', description: '选择或取消一个选项。',
    package: '', source: 'src/components/Checkbox.tsx',
    props: {checked: false},
    fields: [{key: 'checked', label: '选中', kind: 'boolean'}],
  },
];
```

`package` 填真实 npm 包名；项目内无独立包名时可用空字符串。`source` 指向真实实现。清单仅导出可序列化数据，使用 `import type` 引用类型；数据模块的相对 ESM 导入带文件扩展名。不要在 `props` 中保存函数、ReactNode、DOM 或业务服务实例。

参数控件支持 `text`、`number`、`boolean`、`select`、`json`。数值范围来自组件真实约束；选项值放 `options`，中文显示名放 `optionLabels`。动态选项通过 `optionsFrom` 指向参数中含 `{value,label}` 的数组。复杂插槽可以使用 JSON，但需要渲染器实际解析；不能只登记一个不可操作的字段。

## 5. 渲染真实组件并打通参数回传

```tsx
// frame/renderer.tsx
import type {LibraryRenderer} from 'loom-frame';
import {Button} from '../src/components/Button';
import {Checkbox} from '../src/components/Checkbox';
import '../src/styles/components.css';

export const renderer: LibraryRenderer = {
  render: ({entry, props, onPatch}) => {
    switch (entry.id) {
      case 'kit:button':
        return <Button disabled={Boolean(props.disabled)}>
          {String(props.children ?? '')}
        </Button>;
      case 'kit:checkbox':
        return <Checkbox
          checked={Boolean(props.checked)}
          onCheckedChange={(checked: boolean) => onPatch('checked', checked)}
        />;
      default:
        throw new Error('组件尚未提供预览实现：' + entry.id);
    }
  },
  applyTheme: (theme, tokens) => {
    // 此例假设来源 CSS 使用 data-theme 和传入的 CSS 变量。
    document.documentElement.dataset.theme = theme.mode;
    for (const [key, value] of Object.entries(tokens)) {
      document.documentElement.style.setProperty(key, value);
    }
  },
};
```

Checkbox 的回调可能实际是 DOM change 事件、布尔值或多个状态，必须按来源接口转换。表单值、选中项、展开状态等可调参数通过 `onPatch(key,value)` 回到工作台，不能只改局部状态导致画布与参数面板不一致。`onEvent` 当前不提供可见事件日志，不要靠它代替交互结果。

根据源码补齐最小预览容器：主题、国际化、路由或必要 Context。导入真实 CSS、字体、图标与图片，保留原组件结构与样式。弹窗和菜单需要处理挂载容器与开关回传；不能给每个组件统一套一层会裁剪浮层或改变布局的卡片。

网络请求、登录、权限、支付等流程通过接入方提供的演示适配隔离。使用明确的预览数据，操作回调不触发生产写入。无法独立运行的组件要指出具体依赖，再实现适配或说明阻塞，不用相似 HTML 冒充来源组件。

## 6. 对接主题、组合组件和预览能力

先读来源项目的 Token 和主题实现，再编写 `applyTheme`。如果来源需要 ThemeProvider，在渲染容器中使用真实 Provider；如果使用 CSS 变量，将 `theme` 中支持的值映射到对应变量，并应用传入的 `tokens`。示例中的 data-theme 不能直接用于所有项目。

框架的 Theme 包括 mode、accent、surface、text、muted、border、radius、fontSize、motion、borderWidth、shadow。只有实际映射的设置才会影响来源组件，不要宣称所有样式控件天然生效。库的 `defaults` 可分别提供完整的 light / dark Theme；不同组件库各自维护设计映射。

组合条目示例：`kind: 'composite'`、`group: '资料'`，并填写 `basicComponentIds: ['kit:avatar', 'kit:button']`。Avatar 和 Button 必须已登记且确为该组合使用到的基础组件。提供必要子项和演示数据即可，业务页面外壳不属于组件本体。

根据实际实现声明 `FrameLibrary.preview`：

| 能力 | 何时使用 |
| --- | --- |
| `viewport` | 组件有建议预览宽高；可设置 width、height、minWidth |
| `persistent` | 原生会话或其他必须跨组件切换保留的渲染容器 |
| `scene` | 已提供真实 `renderScene` 时才设为 true |
| `states` | 按库是否支持状态矩阵设置；不支持时设 false |
| `highlight` | 按组件是否支持 DOM 元素高亮设置；不支持时设 false |
| `language` | 真实使用代码语言，如 tsx 或 kotlin |

不要仅打开能力开关却没有对应实现。

## 7. 启动与使用 Loom

独立预览：

```sh
npx loom dev
```

默认访问 `http://127.0.0.1:4321/`，端口占用时会报错。目录仍为空时，先看 `frame.config.ts` 是否登记 libraries、清单是否导出 catalog，以及组件库所需插件是否启用。修改组件实现后由开发服务热更新；修改配置后重启开发命令。

工作台中选择组件、修改右侧参数，在画布里操作控件并观察参数回传；切换明暗主题、调整预览尺寸、保存配置或复制参数 JSON。默认配置保存在当前浏览器 IndexedDB，偏好保存在 localStorage，按站点来源和 namespace 隔离；换端口不会自动搬迁数据。

用户需要嵌入已有 Vite 项目时，在原配置中加入插件，保留原来的 React、CSS 和路径别名配置：

```ts
import {defineConfig} from 'vite';
import {framePlugin} from 'loom-frame/vite';
import frameConfig from './frame.config';

export default defineConfig(async () => ({
  // 与项目原有插件合并，不重复添加已有插件。
  plugins: [await framePlugin(frameConfig)],
}));
```

通过项目原有开发命令启动，访问 `/__frame/`。这条路径与独立 CLI 启动方式二选一即可，不需要再占一个开发端口。

用户需要交付静态预览时执行 `npx loom build`，默认产物在 `dist/frame`；`npx loom serve` 提供已构建网页，`npx loom open` 还会打开浏览器。`npx loom lookup kit:button` 从已构建的 components.json 查询组件；源码清单更新后需重新构建才能刷新查询结果。静态目录通过 HTTP/HTTPS 部署，不直接用 file:// 打开。

## 8. Android 项目另走原生接入

需要先了解完整接入样例时，可参考独立的 `loom-ui-android` 包：它提供 `dev.loom.ui` Compose 组件、AAR、协议 4 的预览宿主与匹配的资源。导出的 `loomUIAndroidLibrary()` 登记清单，`loomUIAndroidPlugin({enabled:true})` 连接配套预览。这是可运行的通用示例，不是对其他项目进行自动提取的工具；新组件必须替换为真实实现，并重新构建匹配 ID 的宿主 APK。

先理解项目的原生 UI 技术、组件入口、主题、构建变体及依赖。Loom 不把原生组件重画成 Web，也不能对任意普通 APK 自动提取组件。登记 Kotlin 源码只解决清单和定位，不等于已经能渲染。

原生接入需要以下产物：

1. 独立的原生组件清单，ID 与预览宿主中的组件分发保持一致。
2. 能直接调用真实原生组件的预览宿主 APK，包括 props / 主题应用、输入回传与绘制状态通知。
3. 与当前插件匹配的 manifest.json 和 scrcpy-server，以及本机 Android SDK、模拟器和系统镜像。

安装用户提供的 `loom-frame` 和 `@loom-frame/android` 两个本地包。框架已内含 UI 实现，无需另装 loom-ui。未发布到 registry 时在同一次安装中提供两个 tgz；普通第三方依赖仍需联网获取。按实际资源目录配置：

```sh
npm install -D ./loom-frame-0.1.0.tgz ./loom-frame-android-0.1.0.tgz
```

```ts
import type {FrameConfig} from 'loom-frame';
import {androidPlugin, androidLibrary} from '@loom-frame/android';

export default {
  plugins: [androidPlugin({
    enabled: true,
    assets: './native-assets',
    dataDir: './.frame-android',
  })],
  libraries: [androidLibrary({
    key: 'native-kit', name: 'Native Kit',
    catalog: './frame/native-catalog.ts',
    sourceRoots: ['app/src/main'],
  })],
} satisfies FrameConfig;
```

assets 内文件为 `preview.apk`、`manifest.json`、`scrcpy-server`。当前资源 manifest 使用 protocol 4、scrcpyVersion 4.1，以及 package、activity、componentIds；可选 bootstrapActivity、scrcpyBuild 必须与实际宿主和视频服务一致。不要为了通过启动条件伪造版本或随意填写这些值。

新 Android 项目还需要实现匹配的宿主通信：当前插件使用 ADB 转发的 7831 端口、会话 token 握手，以及 render / 输入 / release 等命令和 patch / painted / moving / still 等事件。完整消息形态与视频几何扩展必须与安装插件版本对应。当前框架没有通用 Android 宿主生成器；缺少宿主或资源时，明确报告还需要完成的原生适配，不把已有产品的 APK 当作通用包交付。

启动仍用 `npx loom dev`，在工作台插件工具条中启用原生预览。Node 服务负责模拟器与视频转发，浏览器负责解码显示。Web 和 Android 的库登记各自独立；普通 Web 不需要安装此插件。静态网页不自动获得本地原生运行时，远程预览还需另行部署设备和受控代理。

## 9. 完成接入与后续同步

按接入项目的约定与用户授权运行必要命令。查看真实预览时，覆盖本次接入的基础与组合组件、参数变更及回传、明暗外观、缩放和浮层交互；发现布局或状态问题时追到共享依赖处理。没有运行或无法观察的部分明确记录，不用构建成功代替可用性结论。

向用户交付：可使用的启动命令和地址、登记的组件范围、清单与渲染器位置、保留或跳过项及其原因、原生额外资源、实际观察过的状态和未完成项。不要只有一份清单而没有渲染实现，也不要只演示一个按钮就宣称整个组件库已完成。

后续组件更新时重新读取来源实现和调用方，增量更新参数、分类、依赖与渲染分发；保留用户已有配置和稳定 ID。接入知识只写来源事实、操作方法与必要限制，不把维护 Loom 仓库时使用的个人规则复制进使用者项目。
