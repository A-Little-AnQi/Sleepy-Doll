# Sleepy Doll Android UI

原生 Jetpack Compose 组件库。正式安卓应用与 Frame 预览宿主直接依赖同一个 Gradle 模块，库本身不依赖 Frame、ADB 或网络服务。

同仓库应用使用 `implementation(project(":ui"))`，独立项目可使用本地 Maven 仓库 `dist/android/maven` 中的 `dev.sleepydoll:ui:0.1.0`。组件包名为 `dev.sleepydoll.ui`，入口需要包裹 `SleepyTheme`。

| 组件类型 | API |
| --- | --- |
| 按钮 | `SleepyButton`、`SleepyIconButton`、`SleepyFloatingActionButton` |
| 输入框 | `SleepyTextField`（单行、多行、密码）、`SleepySearchField` |
| 选择控件 | `SleepySwitch`、`SleepyCheckbox`、`SleepySlider`、`SleepySelect`、`SleepyRadioGroup`、`SleepyFilterChips`、`SleepyStepper`、`SleepyRangeSlider`、`SleepyDatePicker`、`SleepyTimePicker` |
| 导航 | `SleepyTabs`、`SleepyTopAppBar`、`SleepyBottomNavigation`、`SleepyStepIndicator` |
| 弹窗与菜单 | `SleepyDialog`、`SleepyBottomSheet`、`SleepyActionSheet`、`SleepyDropdownMenu` |
| 提示与加载 | `SleepyNotice`、`SleepySnackbar`、`SleepyProgress`、`SleepyCircularProgress`、`SleepySkeleton`、`SleepyEmptyState` |
| 卡片与列表 | `SleepyTaskCard`、`SleepyListItem`、`SleepyStatCard`、`SleepyProfileCard`、`SleepyFileItem`、`SleepyUploadCard` |
| 布局与设置 | `SleepySection`、`SleepySettingRow`、`SleepyDivider`、`SleepyAccordion` |
| 图标、标签和消息 | `SleepyGlyph` / `SleepySymbol`、`SleepyAvatar`、`SleepyBadge`、`SleepyMessageBubble`、`SleepyComposer` |

选择值、展开状态、日期、时间和任务进度由调用方持有，通过回调更新；文件、上传、分享等回调由业务接入，组件不会自行读写文件或发起请求。日期 API 使用 `LocalDate`，时间使用 `LocalTime`，不会把本地日历日期误当成设备时区下的时间戳。

`SleepyTopAppBar` 和 `SleepyBottomNavigation` 不重复添加系统栏 inset，放进宿主页面的安全区域或 `Scaffold` 即可。底部抽屉与日期/时间弹窗使用原生模态层。

在仓库根目录执行 `npm run android:publish` 更新 AAR、源码包和演示 APK；`npm run android:preview` 更新 Frame 宿主。`frame/android-catalog.ts` 定义 45 个预览条目（多行输入为单独条目），参数交互由宿主 `NativeComponents.kt` 回传。

预览工具单独通过 `npm run frame:setup -- <框架tgz路径> <安卓插件tgz路径>` 安装到 `frame/node_modules/`，正式组件库、应用构建和 GitHub 构建无需这些未发布包。预览资源脚本默认使用该位置 `@loom-frame/android` 包内配套的视频服务，按 SHA-256 校验 `loom-flex-3` 动态尺寸协议。不要用系统安装的普通 scrcpy 覆盖它；仅修改 `wm size` 无法同步虚拟显示的物理尺寸，会在调整画布后产生黑边。显式的 `SLEEPY_SCRCPY_SERVER` 覆盖同样需要通过协议版本校验。
