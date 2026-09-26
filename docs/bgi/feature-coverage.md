# BGI 功能与适配覆盖清单

源码事实源：`E:\BetterGIProject\better-genshin-impact`；提交：`7e02dc8cee57f264aa8d5f3efe4f18a956c39611`。

全量索引：319 个源码命令、622 个 AllConfig 设置叶节点、68 个 XAML 页面／窗口、140 个脚本 API 方法、35 个脚本资源模型字段；只读快照中有 0 个桥接口。

本清单区分源码功能、当前运行桥的登记结果与实际验证。可调用只表示契约可提交，不代表游戏任务已完成；当前桥未登记可能是版本差异、对象未注册或真实缺口。源码中的内部事件和编辑器命令也列出，不将它们包装成普通自动化能力。

审计判断与修复记录见 [链路审计](integration-audit.md)。完整结构化数据见 [feature-coverage.json](feature-coverage.json)。

## 源码命令

源码链路分类（非实机通过数量）：`{"可走动态命令链，需参数与结果核验": 218, "原生对象引用或声明类型构造，需上下文绑定": 34, "需核对当前选择，不能仅按名称宣称完成": 24, "需 dialogInput 绑定本次弹窗并核验": 21, "内部事件／空实现，不作为业务入口": 22}`。

| 功能／界面标签 | 源码对象与位置 | 参数 | 桥入口／状态 | 输入与选择依赖 | 链路审查／替代 |
|---|---|---|---|---|---|
| SwitchHotKeyTypeCommand | BetterGenshinImpact.Model.HotKeySettingModel / BetterGenshinImpact/Model/HotKeySettingModel.cs:234 | 无参数 | cmd.hot_key_setting_model.switch_hot_key_type / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| CloseDrawerCommand | BetterGenshinImpact.View.Controls.Drawer.DrawerViewModel / BetterGenshinImpact/View/Controls/Drawer/DrawerViewModel.cs:49 | 无参数 | cmd.drawer.close_drawer / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenDrawerCommand | BetterGenshinImpact.View.Controls.Drawer.DrawerViewModel / BetterGenshinImpact/View/Controls/Drawer/DrawerViewModel.cs:42 | object content | cmd.drawer.open_drawer / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| ToggleDrawerCommand | BetterGenshinImpact.View.Controls.Drawer.DrawerViewModel / BetterGenshinImpact/View/Controls/Drawer/DrawerViewModel.cs:55 | object? content | cmd.drawer.toggle_drawer / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 后台更新 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:121 | 无参数 | cmd.check_update_window.background_update / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 取消 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:244 | 无参数 | cmd.check_update_window.cancel / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| Mirror酱服务 💰：修改CDK | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:253 | 无参数 | cmd.check_update_window.edit_cdk / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 不再提示 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:235 | 无参数 | cmd.check_update_window.ignore / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 手动下载 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:130 | 无参数 | cmd.check_update_window.other_update / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| UpdateCommand | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:139 | 无参数 | cmd.check_update_window.update / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 开源渠道：立即更新 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:148 | 无参数 | cmd.check_update_window.update_from_git_host_platform / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| Mirror酱服务 💰：立即更新 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:181 | 无参数 | cmd.check_update_window.update_from_mirror_chyan / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| Steambird 服务 🆓：立即更新 | BetterGenshinImpact.View.Windows.CheckUpdateWindow / BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:168 | 无参数 | cmd.check_update_window.update_from_steambird / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 选择zip文件导入 | BetterGenshinImpact.View.Windows.ScriptRepoWindow / BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:498 | 无参数 | cmd.script_repo_window.import_local_scripts_repo_zip / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| OpenLocalScriptRepoCommand | BetterGenshinImpact.View.Windows.ScriptRepoWindow / BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:335 | 无参数 | cmd.script_repo_window.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 重置仓库 | BetterGenshinImpact.View.Windows.ScriptRepoWindow / BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:409 | 无参数 | cmd.script_repo_window.reset_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 更新仓库 | BetterGenshinImpact.View.Windows.ScriptRepoWindow / BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:259 | 无参数 | cmd.script_repo_window.update_repo / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 一键更新订阅 | BetterGenshinImpact.View.Windows.ScriptRepoWindow / BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:545 | 无参数 | cmd.script_repo_window.update_subscribed_scripts / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ActivatedCommand | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:164 | 无参数 | cmd.main_window.activated / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| ClosingCommand | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:309 | CancelEventArgs e | cmd.main_window.closing / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 关闭 | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:356 | 无参数 | cmd.main_window.dismiss_redeem_code / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 最小化到托盘 | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:200 | 无参数 | cmd.main_window.hide / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| LoadedCommand | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:379 | 无参数 | cmd.main_window.loaded / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 获取兑换码信息 | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:328 | 无参数 | cmd.main_window.open_feed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 切换深浅主题或背景样式 | BetterGenshinImpact.ViewModel.MainWindowViewModel / BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:206 | 无参数 | cmd.main_window.switch_backdrop / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| CloseCommand | BetterGenshinImpact.ViewModel.MaskMapPointInfoPopupViewModel / BetterGenshinImpact/ViewModel/MaskMapPointInfoPopupViewModel.cs:281 | 无参数 | cmd.mask_map_point_info_popup.close / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenUrlCommand | BetterGenshinImpact.ViewModel.MaskMapPointInfoPopupViewModel / BetterGenshinImpact/ViewModel/MaskMapPointInfoPopupViewModel.cs:305 | string? url | cmd.mask_map_point_info_popup.open_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleHiddenCommand | BetterGenshinImpact.ViewModel.MaskMapPointInfoPopupViewModel / BetterGenshinImpact/ViewModel/MaskMapPointInfoPopupViewModel.cs:275 | 无参数 | cmd.mask_map_point_info_popup.toggle_hidden / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ExitOverlayLayoutEditModeCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:524 | 无参数 | cmd.mask_window.exit_overlay_layout_edit_mode / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 全部隐藏 | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:981 | 无参数 | cmd.mask_window.hide_all_map_points / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| LoadedCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:179 | 无参数 | cmd.mask_window.loaded / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| OverlayLayoutCommittedCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:467 | OverlayLayoutCommittedEventArgs args | cmd.mask_window.overlay_layout_committed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| PointClickCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:921 | MaskMapPointClickArgs? args | cmd.mask_window.point_click / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| PointHoverCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:951 | MaskMapPoint? point | cmd.mask_window.point_hover / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| PointRightClickCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:944 | MaskMapPoint? point | cmd.mask_window.point_right_click / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 重置选择 | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:233 | 无参数 | cmd.mask_window.reset_selected_map_label_selection / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| SelectMapLabelCategoryCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:203 | MapLabelCategoryVm? category | cmd.mask_window.select_map_label_category / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| − | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:210 | MapLabelItemVm? item | cmd.mask_window.select_map_label_item / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 全部显示 | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:1000 | 无参数 | cmd.mask_window.show_all_map_points / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleMapPointHiddenCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:962 | MaskMapPoint? point | cmd.mask_window.toggle_map_point_hidden / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| ToggleMapPointPickerCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:187 | 无参数 | cmd.mask_window.toggle_map_point_picker / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| WindowSizeChangedCommand | BetterGenshinImpact.ViewModel.MaskWindowViewModel / BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:513 | SizeChangedEventArgs args | cmd.mask_window.window_size_changed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| CheckUpdateCommand | BetterGenshinImpact.ViewModel.NotifyIconViewModel / BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:71 | 无参数 | cmd.notify_icon.check_update / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 退出 | BetterGenshinImpact.ViewModel.NotifyIconViewModel / BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:56 | 无参数 | cmd.notify_icon.exit / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenChildSessionWindowCommand | BetterGenshinImpact.ViewModel.NotifyIconViewModel / BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:34 | 无参数 | cmd.notify_icon.open_child_session_window / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ShowOrHideCommand | BetterGenshinImpact.ViewModel.NotifyIconViewModel / BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:40 | 无参数 | cmd.notify_icon.show_or_hide / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 版本更新：检查更新 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:403 | 无参数 | cmd.common_settings_page.check_update_alpha / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 版本更新：检查更新 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:393 | 无参数 | cmd.common_settings_page.check_update / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 主窗口自定义背景：清除 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:491 | 无参数 | cmd.common_settings_page.clear_main_background_image / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 响应「原神游戏语言」的界面事件 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:439 | KeyValuePair<string, string> type | cmd.common_settings_page.game_lang_selection_changed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 开发者功能：log/screenshot | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:314 | 无参数 | cmd.common_settings_page.go_to_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 开发者功能：绑定快捷键 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:297 | 无参数 | cmd.common_settings_page.go_to_hot_key_page / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 显示的指标：打开日志目录 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:338 | 无参数 | cmd.common_settings_page.go_to_log_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 开发者功能：log/RewardRecognition | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:326 | 无参数 | cmd.common_settings_page.go_to_reward_recognition_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 选择zip文件导入 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:350 | 无参数 | cmd.common_settings_page.import_local_scripts_repo_zip / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 关于 BetterGI：查看 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:376 | 无参数 | cmd.common_settings_page.open_about_window / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 显示的指标：打开编辑器 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:252 | 无参数 | cmd.common_settings_page.open_custom_html_mask_editor / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 显示的指标：打开目录 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:258 | 无参数 | cmd.common_settings_page.open_custom_html_mask_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 按键绑定设置：配置 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:384 | 无参数 | cmd.common_settings_page.open_key_bindings_window / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 开发者功能：选择图片 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:303 | 无参数 | cmd.common_settings_page.open_recognition_template_editor_from_image / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 响应「OCR 配置」的界面事件 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:445 | PaddleOcrModelConfig value | cmd.common_settings_page.paddle_ocr_model_config_changed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 米游社相关：? | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:144 | 无参数 | cmd.common_settings_page.question_button_on_click / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 响应「启用遮罩窗口」的界面事件 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:227 | 无参数 | cmd.common_settings_page.refresh_mask_settings / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用遮罩窗口：重置位置 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:265 | 无参数 | cmd.common_settings_page.reset_mask_overlay_layout / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用遮罩窗口：重置 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:240 | 无参数 | cmd.common_settings_page.reset_overlay_style / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用准星：浏览 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:452 | 无参数 | cmd.common_settings_page.select_crosshair_image / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 主窗口自定义背景：选择图片 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:466 | 无参数 | cmd.common_settings_page.select_main_background_image / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 响应「启用遮罩窗口」的界面事件 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:284 | 无参数 | cmd.common_settings_page.switch_mask_enabled / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 响应「开发者功能」的界面事件 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:309 | 无参数 | cmd.common_settings_page.switch_taken_screenshot_enabled / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 响应「软件UI语言」的界面事件 | BetterGenshinImpact.ViewModel.Pages.CommonSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:426 | object? value | cmd.common_settings_page.ui_language_selection_changed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 响应「BetterGI 截图器，启动！」的界面事件 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:206 | 无参数 | cmd.home_page.capture_mode_drop_down_changed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 更换背景图片 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:709 | 无参数 | cmd.home_page.change_banner_image / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 使用网络图片 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:755 | 无参数 | cmd.home_page.change_web_banner_image / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 点击查看文档与教程 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:412 | 无参数 | cmd.home_page.go_to_wiki_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| LoadedCommand | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:153 | 无参数 | cmd.home_page.loaded / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| BetterGI 截图器，启动！：选择捕获窗口 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:243 | 无参数 | cmd.home_page.manual_pick_window / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| BetterGI 桌面分身：打开 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:147 | 无参数 | cmd.home_page.open_child_session_window / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| BetterGI 截图器，启动！：手动设置 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:261 | 无参数 | cmd.home_page.open_display_advanced_graphics_settings / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 同时启动原神：打开文档 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:524 | 无参数 | cmd.home_page.open_game_command_line_document / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| BetterGI 截图器，启动！：更多... | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:575 | 无参数 | cmd.home_page.open_hardware_acceleration_settings / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 刷新网络图片 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:782 | 无参数 | cmd.home_page.refresh_web_banner_image / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 恢复默认图片 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:807 | 无参数 | cmd.home_page.reset_banner_image / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 同时启动原神：浏览 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:455 | 无参数 | cmd.home_page.select_install_path / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| BetterGI 截图器，启动！：测试图像捕获 | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:223 | 无参数 | cmd.home_page.start_capture_test / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| StartTriggerCommand | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:272 | 无参数 | cmd.home_page.start_trigger / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| StopTriggerCommand | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:371 | 无参数 | cmd.home_page.stop_trigger / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| TestCommand | BetterGenshinImpact.ViewModel.Pages.HomePageViewModel / BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:418 | 无参数 | cmd.home_page.test / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 删除 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:139 | ScriptProject? item | cmd.js_list.delete_script / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 点击查看 Javascript 脚本使用与编写教程 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:187 | 无参数 | cmd.js_list.go_to_js_script_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 脚本仓库 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:194 | 无参数 | cmd.js_list.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenScriptDetailDrawerCommand | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:207 | object? scriptItem | cmd.js_list.open_script_detail_drawer / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 打开目录 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:110 | ScriptProject? item | cmd.js_list.open_script_project_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 打开脚本目录 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:104 | 无参数 | cmd.js_list.open_scripts_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 刷新 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:133 | ScriptProject? item | cmd.js_list.refresh / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| SetRightClickSelectionCommand | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:201 | string isRightClick | cmd.js_list.set_right_click_selection / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 执行脚本 | BetterGenshinImpact.ViewModel.Pages.JsListViewModel / BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:116 | ScriptProject? item | cmd.js_list.start_run / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定；bgi.prepare_js_group + bgi.run_script_group |
| 从注册表读取 | BetterGenshinImpact.ViewModel.Pages.KeyBindingsSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyBindingsSettingsPageViewModel.cs:373 | 无参数 | cmd.key_bindings_settings_page.fetch_from_registry / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 删除 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:190 | KeyMouseScriptItem? item | cmd.key_mouse_record_page.delete_script / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 修改名称 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:146 | KeyMouseScriptItem? item | cmd.key_mouse_record_page.edit_script / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 点击查看键鼠录制回放功能教程 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:224 | 无参数 | cmd.key_mouse_record_page.go_to_km_script_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 脚本仓库 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:230 | 无参数 | cmd.key_mouse_record_page.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 打开脚本目录 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:140 | 无参数 | cmd.key_mouse_record_page.open_script_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 播放脚本 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:118 | string path | cmd.key_mouse_record_page.start_play / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 开始录制 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:81 | 无参数 | cmd.key_mouse_record_page.start_record / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 停止录制 | BetterGenshinImpact.ViewModel.Pages.KeyMouseRecordPageViewModel / BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:96 | 无参数 | cmd.key_mouse_record_page.stop_record / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 一键宏（按角色）：前往设置 | BetterGenshinImpact.ViewModel.Pages.MacroSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/MacroSettingsPageViewModel.cs:64 | 无参数 | cmd.macro_settings_page.edit_avatar_macro / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 一键领取奖励：绑定快捷键 | BetterGenshinImpact.ViewModel.Pages.MacroSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/MacroSettingsPageViewModel.cs:58 | 无参数 | cmd.macro_settings_page.go_to_hot_key_page / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 一键宏（按角色）：点击查看说明 | BetterGenshinImpact.ViewModel.Pages.MacroSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/MacroSettingsPageViewModel.cs:70 | 无参数 | cmd.macro_settings_page.go_to_one_key_macro_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 删除 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:199 | FileTreeNode<PathingTask>? item | cmd.map_pathing.delete / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 点击查看地图追踪与录制使用教程 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:187 | 无参数 | cmd.map_pathing.go_to_pathing_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 开发者工具 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:155 | 无参数 | cmd.map_pathing.open_dev_tools / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 脚本仓库 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:280 | 无参数 | cmd.map_pathing.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenPathingDetailCommand | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:293 | 无参数 | cmd.map_pathing.open_pathing_detail / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 打开目录 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:114 | ScriptProject? item | cmd.map_pathing.open_script_project_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 打开任务目录 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:103 | 无参数 | cmd.map_pathing.open_scripts_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 设置 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:170 | 无参数 | cmd.map_pathing.open_settings / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 刷新 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:193 | 无参数 | cmd.map_pathing.refresh / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SetRightClickSelectionCommand | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:287 | string isRightClick | cmd.map_pathing.set_right_click_selection / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 执行任务 | BetterGenshinImpact.ViewModel.Pages.MapPathingViewModel / BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:125 | FileTreeNode<PathingTask>? item | cmd.map_pathing.start / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| BeginSeekCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:542 | 无参数 | cmd.music_page.begin_seek / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 选择目录 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:331 | 无参数 | cmd.music_page.choose_folder / 源码已列出，运行桥尚未核验 | 弹窗输入；当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| CyclePlaybackModeCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:320 | 无参数 | cmd.music_page.cycle_playback_mode / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 删除记录 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:395 | string? folder | cmd.music_page.delete_music_folder / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| InitializeCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:219 | 无参数 | cmd.music_page.initialize / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 下一首 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:530 | 无参数 | cmd.music_page.next / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 打开当前目录 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:435 | 无参数 | cmd.music_page.open_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 设置 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:462 | 无参数 | cmd.music_page.open_settings / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 播放/暂停 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:472 | 无参数 | cmd.music_page.play_pause / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| PlaySelectedCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:491 | PerformanceScore? musicItem | cmd.music_page.play_selected / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 上一首 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:536 | 无参数 | cmd.music_page.previous / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| RefreshCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:455 | 无参数 | cmd.music_page.refresh / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| RefreshMappingCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:570 | 无参数 | cmd.music_page.refresh_mapping / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 保存映射 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:588 | 无参数 | cmd.music_page.save_profiles / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SeekCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:548 | double milliseconds | cmd.music_page.seek / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| SelectMusicFolderCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:350 | string? folder | cmd.music_page.select_music_folder / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 停止 | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:519 | 无参数 | cmd.music_page.stop / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| UpdateTrackSelectionCommand | BetterGenshinImpact.ViewModel.Pages.MusicPageViewModel / BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:581 | 无参数 | cmd.music_page.update_track_selection / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 绑定 QQ 群按钮。连接 QQ 网关，等待用户将机器人加入群聊或发送验证码，自动回填群 OpenID。 支持取消（用户点击取消时中断 WebSocket 连接）。 超时（60 秒）时提示用户重试。 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:765 | 无参数 | cmd.notification_settings_page.bind_group_qq / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 绑定 QQ 按钮。连接 QQ 网关，等待用户发送验证码，自动回填 OpenID。 支持取消（用户点击取消时中断 WebSocket 连接）。 超时（60 秒未收到消息）时提示用户重试。 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:600 | 无参数 | cmd.notification_settings_page.bind_qq / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 微信 Clawbot 登录并绑定按钮。扫码登录获取 bot_token，再等待用户发送一次性验证码， 自动回填 to_user_id 和 context_token。支持取消。 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:671 | 无参数 | cmd.notification_settings_page.bind_wechat_clawbot / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 取消群 QQ 绑定按钮：取消 WebSocket 连接，中断绑定流程。 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:833 | 无参数 | cmd.notification_settings_page.cancel_bind_group_qq / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 取消绑定按钮：取消 WebSocket 连接，中断绑定流程。 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:661 | 无参数 | cmd.notification_settings_page.cancel_bind_qq / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 取消微信 Clawbot 登录/绑定按钮。 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:754 | 无参数 | cmd.notification_settings_page.cancel_bind_wechat_clawbot / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 全局通知设置：取消选择 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:158 | 无参数 | cmd.notification_settings_page.clear_notification_event_selection / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 全局通知设置：打开文档 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:839 | 无参数 | cmd.notification_settings_page.open_notification_event_document / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 全局通知设置：全选 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:152 | 无参数 | cmd.notification_settings_page.select_all_notification_events / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 Bark 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:412 | 无参数 | cmd.notification_settings_page.test_bark_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用钉钉机器人通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:469 | 无参数 | cmd.notification_settings_page.test_ding_ding_webhook_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 Discord Webhook 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:488 | 无参数 | cmd.notification_settings_page.test_discord_webhook_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用邮箱通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:393 | 无参数 | cmd.notification_settings_page.test_email_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用飞书通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:317 | 无参数 | cmd.notification_settings_page.test_feishu_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 Gotify 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:544 | 无参数 | cmd.notification_settings_page.test_gotify_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 MeoW 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:526 | 无参数 | cmd.notification_settings_page.test_meow_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 OneBot 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:336 | 无参数 | cmd.notification_settings_page.test_one_bot_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 QQ 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:561 | 无参数 | cmd.notification_settings_page.test_qq_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 ServerChan 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:507 | 无参数 | cmd.notification_settings_page.test_server_chan_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 Telegram 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:431 | 无参数 | cmd.notification_settings_page.test_telegram_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 WebSocket：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:374 | 无参数 | cmd.notification_settings_page.test_web_socket_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 Webhook：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:279 | 无参数 | cmd.notification_settings_page.test_webhook / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用微信 Clawbot 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:578 | 无参数 | cmd.notification_settings_page.test_wechat_clawbot_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 Windows 通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:298 | 无参数 | cmd.notification_settings_page.test_windows_uwp_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用企业微信通知：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:355 | 无参数 | cmd.notification_settings_page.test_work_weixin_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启用 xxtui 信息推送：发送 | BetterGenshinImpact.ViewModel.Pages.NotificationSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:450 | 无参数 | cmd.notification_settings_page.test_xxtui_notification / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| AddConfigCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:859 | 无参数 | cmd.one_dragon_flow.add_config / 源码已列出，运行桥尚未核验 | 弹窗输入；当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 新增配置 | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:429 | 无参数 | cmd.one_dragon_flow.add_task_group / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 清除标记 | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:841 | 无参数 | cmd.one_dragon_flow.clear_next_task_group / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| ConfigDropDownChangedCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:402 | 无参数 | cmd.one_dragon_flow.config_drop_down_changed / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| CopyTaskCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:747 | OneDragonTaskItem? taskItem | cmd.one_dragon_flow.copy_task / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| DeleteConfigCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:882 | 无参数 | cmd.one_dragon_flow.delete_config / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| DeleteConfigDisplayTaskListFromConfigCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:385 | 无参数 | cmd.one_dragon_flow.delete_config_display_task_list_from_config / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| DeleteTaskCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:769 | OneDragonTaskItem? taskItem | cmd.one_dragon_flow.delete_task / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 删除 | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:803 | 无参数 | cmd.one_dragon_flow.delete_task_group / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| LoadedCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:534 | 无参数 | cmd.one_dragon_flow.loaded / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 从此执行 | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:811 | 无参数 | cmd.one_dragon_flow.next_task_group / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| OneKeyExecuteCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:572 | 无参数 | cmd.one_dragon_flow.one_key_execute / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成；bgi.run_one_dragon |
| RenameConfigCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:953 | 无参数 | cmd.one_dragon_flow.rename_config / 源码已列出，运行桥尚未核验 | 弹窗输入；当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 自动首领讨伐配置：清空累计 | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:453 | 无参数 | cmd.one_dragon_flow.reset_auto_boss_completed_run_count / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| SaveActionConfigCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:440 | 无参数 | cmd.one_dragon_flow.save_action_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SetTaskAsNextCommand | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:779 | OneDragonTaskItem? taskItem | cmd.one_dragon_flow.set_task_as_next / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 响应「自动首领讨伐配置」的界面事件 | BetterGenshinImpact.ViewModel.Pages.OneDragonFlowViewModel / BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:447 | string type | cmd.one_dragon_flow.strategy_drop_down_opened / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| JS脚本 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:729 | 无参数 | cmd.script_control.add_js_script / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 键鼠脚本 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:840 | 无参数 | cmd.script_control.add_km_script / 源码已列出，运行桥尚未核验 | 弹窗输入；当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 下一次任务从此处执行 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1587 | ScriptGroupProject? item | cmd.script_control.add_next_flag / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 地图追踪任务 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:871 | 无参数 | cmd.script_control.add_pathing / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 新增组 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:85 | 无参数 | cmd.script_control.add_script_group / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 连续任务从此开始执行 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:590 | ScriptGroup? item | cmd.script_control.add_script_group_next_flag / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| Shell | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:861 | 无参数 | cmd.script_control.add_shell / 源码已列出，运行桥尚未核验 | 弹窗输入；当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 清空 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:119 | 无参数 | cmd.script_control.clear_tasks / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 继续执行 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:2120 | 无参数 | cmd.script_control.continue_multi_script_group / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 复制组 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:605 | ScriptGroup? item | cmd.script_control.copy_script_group / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 根据文件夹移除 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1683 | ScriptGroupProject? item | cmd.script_control.delete_script_by_folder / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 移除 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1712 | ScriptGroupProject? item | cmd.script_control.delete_script / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 删除组 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:696 | ScriptGroup? item | cmd.script_control.delete_script_group / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定；bgi.delete_script_group |
| 修改JS脚本自定义配置 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1628 | ScriptGroupProject? item | cmd.script_control.edit_js_script_settings / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 修改通用配置 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1571 | ScriptGroupProject? item | cmd.script_control.edit_script_common / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 导出根据控制文件修改任务 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:562 | 无参数 | cmd.script_control.export_merger_jsons / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 点击查看调度器使用教程 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1937 | 无参数 | cmd.script_control.go_to_script_group_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ImportScriptGroupCommand | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1943 | string scriptGroupExample | cmd.script_control.import_script_group / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 打开脚本仓库 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:498 | 无参数 | cmd.script_control.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 日志分析 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:138 | 无参数 | cmd.script_control.open_log_parse / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 打开所在目录 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1730 | ScriptGroupProject? item | cmd.script_control.open_script_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 设置 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1995 | 无参数 | cmd.script_control.open_script_group_settings / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 重命名 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:646 | ScriptGroup? item | cmd.script_control.rename_script_group / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 任务倒序排列 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:553 | 无参数 | cmd.script_control.reverse_task_order / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 连续执行 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:2258 | 无参数 | cmd.script_control.start_multi_script_group / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验；bgi.run_script_group |
| 运行 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1968 | 无参数 | cmd.script_control.start_script_group / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 根据文件夹更新 | BetterGenshinImpact.ViewModel.Pages.ScriptControlViewModel / BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:505 | 无参数 | cmd.script_control.update_tasks / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成 |
| 自动分解圣遗物：从脚本仓库复制 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:814 | 无参数 | cmd.task_settings_page.copy_artifact_salvage_java_script_from_repository / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 自动分解圣遗物：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:801 | 无参数 | cmd.task_settings_page.go_to_artifact_salvage_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动秘境：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:554 | 无参数 | cmd.task_settings_page.go_to_auto_domain_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动战斗：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:480 | 无参数 | cmd.task_settings_page.go_to_auto_fight_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 全自动钓鱼（单个鱼塘）：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:768 | 无参数 | cmd.task_settings_page.go_to_auto_fishing_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动七圣召唤：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:436 | 无参数 | cmd.task_settings_page.go_to_auto_genius_invokation_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动地脉花：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:582 | 无参数 | cmd.task_settings_page.go_to_auto_ley_line_outcrop_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动千音雅集：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:674 | 无参数 | cmd.task_settings_page.go_to_auto_music_game_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动幽境危战：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:576 | 无参数 | cmd.task_settings_page.go_to_auto_stygian_onslaught_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| GoToAutoTrackPathUrlCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:659 | 无参数 | cmd.task_settings_page.go_to_auto_track_path_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| GoToAutoTrackUrlCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:624 | 无参数 | cmd.task_settings_page.go_to_auto_track_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动伐木：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:451 | 无参数 | cmd.task_settings_page.go_to_auto_wood_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 截取物品图标（开发者）：log/gridIcons | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:874 | 无参数 | cmd.task_settings_page.go_to_get_grid_icons_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 截取物品图标（开发者）：点击查看使用教程 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:886 | 无参数 | cmd.task_settings_page.go_to_get_grid_icons_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| GoToHotKeyPageCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:395 | 无参数 | cmd.task_settings_page.go_to_hot_key_page / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 截取物品图标（开发者）：打开目录 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:922 | 无参数 | cmd.task_settings_page.go_to_inventory_count_comparison_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| GoToTorchPreviousVersionsCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:774 | 无参数 | cmd.task_settings_page.go_to_torch_previous_versions / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动分解圣遗物：打开测试窗口 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:807 | 无参数 | cmd.task_settings_page.open_artifact_salvage_test_ocrwindow / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动战斗：打开目录 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:589 | 无参数 | cmd.task_settings_page.open_fight_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动七圣召唤：脚本仓库 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:780 | 无参数 | cmd.task_settings_page.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启动当前选择目标的数量 OCR 对比任务。 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:907 | 无参数 | cmd.task_settings_page.run_inventory_count_comparison / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SOneDragonFlowCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:349 | 无参数 | cmd.task_settings_page.sone_dragon_flow / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需核对当前选择，不能仅按名称宣称完成；bgi.run_one_dragon |
| StopSoloTaskCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:364 | 无参数 | cmd.task_settings_page.stop_solo_task / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验；bgi.stop_current_task |
| 响应「自动七圣召唤」的界面事件 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:389 | string type | cmd.task_settings_page.strategy_drop_down_opened / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchArtifactSalvageCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:786 | 无参数 | cmd.task_settings_page.switch_artifact_salvage / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoAlbumCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:680 | 无参数 | cmd.task_settings_page.switch_auto_album / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoBossCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:538 | 无参数 | cmd.task_settings_page.switch_auto_boss / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoComboCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:698 | 无参数 | cmd.task_settings_page.switch_auto_combo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 响应「自动连招（实验）」的界面事件 | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:707 | 无参数 | cmd.task_settings_page.switch_auto_combo_run / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoCookCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:689 | 无参数 | cmd.task_settings_page.switch_auto_cook / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoDomainCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:486 | 无参数 | cmd.task_settings_page.switch_auto_domain / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoFightCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:457 | 无参数 | cmd.task_settings_page.switch_auto_fight / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoFishingCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:747 | 无参数 | cmd.task_settings_page.switch_auto_fishing / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoGeniusInvokationCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:401 | 无参数 | cmd.task_settings_page.switch_auto_genius_invokation / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoLeyLineOutcropCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:757 | 无参数 | cmd.task_settings_page.switch_auto_ley_line_outcrop / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoMusicGameCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:665 | 无参数 | cmd.task_settings_page.switch_auto_music_game / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoRedeemCodeCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:934 | 无参数 | cmd.task_settings_page.switch_auto_redeem_code / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoStygianOnslaughtCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:560 | 无参数 | cmd.task_settings_page.switch_auto_stygian_onslaught / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchAutoTrackCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:595 | 无参数 | cmd.task_settings_page.switch_auto_track / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| SwitchAutoTrackPathCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:630 | 无参数 | cmd.task_settings_page.switch_auto_track_path / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| SwitchAutoWoodCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:442 | 无参数 | cmd.task_settings_page.switch_auto_wood / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchGetGridIconsCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:860 | 无参数 | cmd.task_settings_page.switch_get_grid_icons / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SwitchGridIconsModelAccuracyTestCommand | BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:892 | 无参数 | cmd.task_settings_page.switch_grid_icons_model_accuracy_test / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 响应「自动拾取」的界面事件 | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:94 | AutoPickMode mode | cmd.trigger_settings_page.auto_pick_mode_changed / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 冷却提示：前往配置 | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:128 | 无参数 | cmd.trigger_settings_page.edit_skill_cd_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 快速传送：[手动触发快速传送触发快捷键] | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:153 | 无参数 | cmd.trigger_settings_page.go_to_hot_key_page / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动拾取：配置规则 | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:74 | 无参数 | cmd.trigger_settings_page.open_blacklist_mode_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自动拾取：配置规则 | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:84 | 无参数 | cmd.trigger_settings_page.open_whitelist_mode_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| [RelayCommand] private void OnOpenReExploreCharacterBox(object sender) { var str = PromptDialog.Prompt("请使用派遣界面展示的角色名，英文逗号分割，从左往右优先级依次降低。\n示例：菲谢尔,班尼特,夜兰,申鹤,久岐忍", "派遣角色优先级配置", Config.AutoSkipConfig.AutoReExploreCharacter); Config.AutoSkipConfig.AutoReExploreCharacter = str.Replace("，", ",").Replace(" ", ""); } | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:116 | SkillCdRule rule | cmd.trigger_settings_page.remove_skill_cd_rule / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 响应「自动剧情」的界面事件 | BetterGenshinImpact.ViewModel.Pages.TriggerSettingsPageViewModel / BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:147 | 无参数 | cmd.trigger_settings_page.toggle_voice_diagnostic_recording / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenFightFolderCommand | BetterGenshinImpact.ViewModel.Pages.View.AutoFightViewModel / BetterGenshinImpact/ViewModel/Pages/View/AutoFightViewModel.cs:120 | 无参数 | cmd.auto_fight.open_fight_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenLocalScriptRepoCommand | BetterGenshinImpact.ViewModel.Pages.View.AutoFightViewModel / BetterGenshinImpact/ViewModel/Pages/View/AutoFightViewModel.cs:113 | 无参数 | cmd.auto_fight.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| StrategyDropDownOpenedCommand | BetterGenshinImpact.ViewModel.Pages.View.AutoFightViewModel / BetterGenshinImpact/ViewModel/Pages/View/AutoFightViewModel.cs:97 | string type | cmd.auto_fight.strategy_drop_down_opened / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 推理设备配置：打开缓存目录 | BetterGenshinImpact.ViewModel.Pages.View.HardwareAccelerationViewModel / BetterGenshinImpact/ViewModel/Pages/View/HardwareAccelerationViewModel.cs:28 | 无参数 | cmd.hardware_acceleration.open_cache_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| + 添加条件 | BetterGenshinImpact.ViewModel.Pages.View.PathingConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:39 | 无参数 | cmd.pathing_config.add_avatar_condition_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| + 添加条件 | BetterGenshinImpact.ViewModel.Pages.View.PathingConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:24 | 无参数 | cmd.pathing_config.add_party_condition_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ClosingCommand | BetterGenshinImpact.ViewModel.Pages.View.PathingConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:57 | CancelEventArgs e | cmd.pathing_config.closing / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 删除 | BetterGenshinImpact.ViewModel.Pages.View.PathingConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:48 | object? item | cmd.pathing_config.remove_avatar_condition_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 删除 | BetterGenshinImpact.ViewModel.Pages.View.PathingConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:30 | object? item | cmd.pathing_config.remove_party_condition_config / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 响应「战斗配置」的界面事件 | BetterGenshinImpact.ViewModel.Pages.View.ScriptGroupConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:141 | 无参数 | cmd.script_group_config.auto_fight_enabled_checked / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 执行周期配置：计算 | BetterGenshinImpact.ViewModel.Pages.View.ScriptGroupConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:121 | 无参数 | cmd.script_group_config.get_execution_order / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 供JS脚本调用Buff类食物配置；食物名称会忽略“美味的”等前缀，请填不带前缀的名称：点击查看调用方法 | BetterGenshinImpact.ViewModel.Pages.View.ScriptGroupConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:147 | 无参数 | cmd.script_group_config.go_to_auto_eat_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 战斗配置：打开目录 | BetterGenshinImpact.ViewModel.Pages.View.ScriptGroupConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:135 | 无参数 | cmd.script_group_config.open_fight_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 战斗配置：脚本仓库 | BetterGenshinImpact.ViewModel.Pages.View.ScriptGroupConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:116 | 无参数 | cmd.script_group_config.open_local_script_repo / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 响应「战斗配置」的界面事件 | BetterGenshinImpact.ViewModel.Pages.View.ScriptGroupConfigViewModel / BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:110 | string type | cmd.script_group_config.strategy_drop_down_opened / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SaveCommand | BetterGenshinImpact.ViewModel.Windows.AutoPickBlacklistConfigViewModel / BetterGenshinImpact/ViewModel/Windows/AutoPickBlacklistConfigViewModel.cs:33 | 无参数 | cmd.auto_pick_blacklist_config.save / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 取消 | BetterGenshinImpact.ViewModel.Windows.AutoPickConfigWindowViewModelBase / BetterGenshinImpact/ViewModel/Windows/AutoPickConfigWindowViewModelBase.cs:26 | 无参数 | cmd.auto_pick_config_window_view_model_base.cancel / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SaveCommand | BetterGenshinImpact.ViewModel.Windows.AutoPickWhitelistConfigViewModel / BetterGenshinImpact/ViewModel/Windows/AutoPickWhitelistConfigViewModel.cs:29 | 无参数 | cmd.auto_pick_whitelist_config.save / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 隐藏 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:247 | 无参数 | cmd.child_session_window.hide / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 运行 BetterGI | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:259 | 无参数 | cmd.child_session_window.launch_better_gi / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 以管理员权限启动… | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:421 | 无参数 | cmd.child_session_window.launch_executable / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 查看帮助文档 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:454 | 无参数 | cmd.child_session_window.open_desktop_help / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 1920 × 1080 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:265 | 无参数 | cmd.child_session_window.select_default_resolution / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 发送 Win+D 显示桌面 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:409 | 无参数 | cmd.child_session_window.show_desktop / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 发送 Win+Tab 切换窗口 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:415 | 无参数 | cmd.child_session_window.show_task_view / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 启动 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:213 | 无参数 | cmd.child_session_window.start / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 切换窗口 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:253 | 无参数 | cmd.child_session_window.switch_window / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleAudioMutedCommand | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:372 | 无参数 | cmd.child_session_window.toggle_audio_muted / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleGameMouseModeCommand | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:353 | 无参数 | cmd.child_session_window.toggle_game_mouse_mode / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 保持宽高比 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:306 | 无参数 | cmd.child_session_window.toggle_keep_aspect_ratio / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 系统组合键发送到桌面分身 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:317 | 无参数 | cmd.child_session_window.toggle_send_system_shortcuts_to_remote / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleSmallWindowModeCommand | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:299 | 无参数 | cmd.child_session_window.toggle_small_window_mode / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleTopmostCommand | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:443 | 无参数 | cmd.child_session_window.toggle_topmost / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 自适应 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:271 | 无参数 | cmd.child_session_window.use_adaptive / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 1 : 1 | BetterGenshinImpact.ViewModel.Windows.ChildSessionWindowViewModel / BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:285 | 无参数 | cmd.child_session_window.use_one_to_one / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| CloseCommand | BetterGenshinImpact.ViewModel.Windows.CustomHtmlMaskEditorViewModel / BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:129 | 无参数 | cmd.custom_html_mask_editor.close / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| OpenCustomHtmlMaskFolderCommand | BetterGenshinImpact.ViewModel.Windows.CustomHtmlMaskEditorViewModel / BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:99 | 无参数 | cmd.custom_html_mask_editor.open_custom_html_mask_folder / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 恢复默认 HTML | BetterGenshinImpact.ViewModel.Windows.CustomHtmlMaskEditorViewModel / BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:82 | 无参数 | cmd.custom_html_mask_editor.restore_default_custom_html_mask / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 保存 HTML | BetterGenshinImpact.ViewModel.Windows.CustomHtmlMaskEditorViewModel / BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:45 | 无参数 | cmd.custom_html_mask_editor.save_custom_html_mask / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| ToggleCustomHtmlMaskPreviewCommand | BetterGenshinImpact.ViewModel.Windows.CustomHtmlMaskEditorViewModel / BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:61 | 无参数 | cmd.custom_html_mask_editor.toggle_custom_html_mask_preview / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 一键兑换 | BetterGenshinImpact.ViewModel.Windows.FeedWindowViewModel / BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:137 | FeedItem item | cmd.feed_window.auto_redeem_item / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 复制 | BetterGenshinImpact.ViewModel.Windows.FeedWindowViewModel / BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:118 | FeedItem item | cmd.feed_window.copy_item_codes / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| 实时获取前瞻兑换码 | BetterGenshinImpact.ViewModel.Windows.FeedWindowViewModel / BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:44 | 无参数 | cmd.feed_window.get_live_redeem_codes / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| RefreshCommand | BetterGenshinImpact.ViewModel.Windows.FeedWindowViewModel / BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:98 | 无参数 | cmd.feed_window.refresh / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 国服 | BetterGenshinImpact.ViewModel.Windows.FeedWindowViewModel / BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:104 | 无参数 | cmd.feed_window.select_cn_server / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 国际服 | BetterGenshinImpact.ViewModel.Windows.FeedWindowViewModel / BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:111 | 无参数 | cmd.feed_window.select_global_server / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 默认添加至头部 | BetterGenshinImpact.ViewModel.Windows.FormViewModel / BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:29 | T item | cmd.form.add / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 原生对象引用或声明类型构造，需上下文绑定 |
| EditAtCommand | BetterGenshinImpact.ViewModel.Windows.FormViewModel / BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:41 | int index | cmd.form.edit_at / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| RemoveAtCommand | BetterGenshinImpact.ViewModel.Windows.FormViewModel / BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:35 | int index | cmd.form.remove_at / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SaveCommand | BetterGenshinImpact.ViewModel.Windows.FormViewModel / BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:52 | 无参数 | cmd.form.save / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| CloseCommand | BetterGenshinImpact.ViewModel.Windows.JsonMonoViewModel / BetterGenshinImpact/ViewModel/Windows/JsonMonoViewModel.cs:63 | 无参数 | cmd.json_mono.close / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SaveCommand | BetterGenshinImpact.ViewModel.Windows.JsonMonoViewModel / BetterGenshinImpact/ViewModel/Windows/JsonMonoViewModel.cs:39 | 无参数 | cmd.json_mono.save / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| DropDownChangedCommand | BetterGenshinImpact.ViewModel.Windows.MapPathingDevViewModel / BetterGenshinImpact/ViewModel/Windows/MapPathingDevViewModel.cs:26 | 无参数 | cmd.map_pathing_dev.drop_down_changed / 源码已列出，运行桥尚未核验 | 当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 内部事件／空实现，不作为业务入口 |
| 录制编辑器 | BetterGenshinImpact.ViewModel.Windows.MapPathingDevViewModel / BetterGenshinImpact/ViewModel/Windows/MapPathingDevViewModel.cs:47 | 无参数 | cmd.map_pathing_dev.open_map_editor / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 查看实时追踪地图 | BetterGenshinImpact.ViewModel.Windows.MapPathingDevViewModel / BetterGenshinImpact/ViewModel/Windows/MapPathingDevViewModel.cs:32 | 无参数 | cmd.map_pathing_dev.open_map_viewer / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 选择… | BetterGenshinImpact.ViewModel.Windows.RecognitionTemplateEditorViewModel / BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:325 | 无参数 | cmd.recognition_template_editor.browse_assets_root / 源码已列出，运行桥尚未核验 | 弹窗输入；当前选择；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 浏览… | BetterGenshinImpact.ViewModel.Windows.RecognitionTemplateEditorViewModel / BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:297 | 无参数 | cmd.recognition_template_editor.browse_recognition_json / 源码已列出，运行桥尚未核验 | 弹窗输入；当前没有可连接的桥快照，不将未核验记为缺失。 | 需 dialogInput 绑定本次弹窗并核验 |
| 取消 | BetterGenshinImpact.ViewModel.Windows.RecognitionTemplateEditorViewModel / BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:441 | 无参数 | cmd.recognition_template_editor.cancel / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| 适应窗口 | BetterGenshinImpact.ViewModel.Windows.RecognitionTemplateEditorViewModel / BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:345 | 无参数 | cmd.recognition_template_editor.fit_image / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| NormalizeTemplateFileNameCommand | BetterGenshinImpact.ViewModel.Windows.RecognitionTemplateEditorViewModel / BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:447 | 无参数 | cmd.recognition_template_editor.normalize_template_file_name / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SaveCommand | BetterGenshinImpact.ViewModel.Windows.RecognitionTemplateEditorViewModel / BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:351 | 无参数 | cmd.recognition_template_editor.save / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |
| SubmitWebImageUrlCommand | BetterGenshinImpact.ViewModel.Windows.WebImageInputViewModel / BetterGenshinImpact/ViewModel/Windows/WebImageInputViewModel.cs:44 | 无参数 | cmd.web_image_input.submit_web_image_url / 源码已列出，运行桥尚未核验 | 当前没有可连接的桥快照，不将未核验记为缺失。 | 可走动态命令链，需参数与结果核验 |

## 全局设置

| 设置路径 | 含义 | 类型 | 源码位置 | 桥状态／联动 |
|---|---|---|---|---|
| autoArtifactSalvageConfig.artifactSetFilter | JavaScript | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:17 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoArtifactSalvageConfig.javaScript | JavaScript | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:10 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoArtifactSalvageConfig.maxArtifactStar | 快速分解圣遗物的最大星级 1~4 | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoArtifactSalvageConfig.maxNumToCheck | 最多检查多少个圣遗物 | int | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:31 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoArtifactSalvageConfig.recognitionFailurePolicy | 单次识别失败策略 | RecognitionFailurePolicy | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:35 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoArtifactSalvageConfig.regularExpression | 正则表达式 | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.bossName |  | string | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:12 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.returnToStatueAfterEachRound | 每轮讨伐后返回七天神像；开启后每次领奖后先回血，再重新前往首领 | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:36 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.reviveRetryCount | 角色死亡后重试次数；战斗中存在角色死亡时，复活后重新讨伐当前首领 | int | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| autoBossConfig.rewardRecognitionEnabled | 是否启用奖励名称识别。默认关闭。 | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:42 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.runCount | 讨伐次数： | int | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:24 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| autoBossConfig.specifyRunCount | 指定讨伐次数；关闭时刷取至原粹树脂耗尽，开启后按成功领取奖励次数停止 | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| autoBossConfig.strategyName | 选择战斗策略；仅用于首领讨伐，不覆盖其他策略设置 | string | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.teamName | 切换队伍；留空则不更换队伍 | string | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:18 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.timeout | 战斗超时 | int | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:48 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.useFragileResin | 原粹不足时使用脆弱树脂补充： | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:30 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoBossConfig.useTransientResin | 原粹不足时使用须臾树脂补充： | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoComboBuildConfig.apiKey | llm服务密钥 | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoComboBuildConfig.extraPrompt | 注入给建树 LLM 的额外提示词（追加在系统指令末尾），留空则不注入 | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:32 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoComboBuildConfig.modelName | 向服务请求的模型名 To learn more about the available models, see https://platform.openai.com/docs/models. | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoComboBuildConfig.planningLlmEndpoint | 决策模型的 OpenAI 兼容端点 | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:13 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoCookConfig.checkIntervalMs | 检测间隔（毫秒）；每次截图检测的时间间隔，最小 1ms | int | BetterGenshinImpact/GameTask/AutoCook/AutoCookConfig.cs:9 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoCookConfig.stopTaskWhenRecoverButtonDetected | 自动结束烹饪任务；开启后检测到“自动烹饪”按钮会点击并结束当前任务 | bool | BetterGenshinImpact/GameTask/AutoCook/AutoCookConfig.cs:12 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.autoArtifactSalvage | 结束后是否自动分解圣遗物 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.autoEat | 自动吃药 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:37 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.condensedResinUseCount | 使用浓缩树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:80 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.domainName | 需要刷取的副本名称 | string | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.fightEndDelay | 战斗结束后延迟几秒再开始寻找石化古树，秒 | double | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:13 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.fragileResinUseCount | 使用脆弱树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:88 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.leftRightMoveTimes | 寻找古树时，短距离移动的次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:31 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.originalResin20UseCount | 使用原粹树脂(20)刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:72 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.originalResin40UseCount | 使用原粹树脂(40)刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:76 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.originalResinUseCount | 使用原粹树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:69 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.partyName | 刷副本使用的队伍名称 | string | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:41 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.resinPriorityList | 自定义使用树脂优先级 | List<string> | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:61 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.reviveRetryCount | 战斗死亡后重试次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:92 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.rewardRecognitionEnabled | 是否启用奖励名称识别。默认关闭。 开启后每轮领取奖励时会用 ONNX 图标匹配 + OCR 材料名双路识别奖励名称与数量，秘境结束打印汇总。 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:99 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.shortMovement | 寻找古树时，短距离移动，用于识别速度过慢的计算机使用 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.specifyResinUse | 指定树脂的使用次数 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:57 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.sundaySelectedValue | 周日奖励序号 | string | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:53 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.transientResinUseCount | 使用须臾树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:84 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoDomainConfig.walkToF | 寻找古树时，短距离移动，用于识别速度过慢的计算机使用 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:25 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.checkInterval | 检测间隔时间（毫秒） | int | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.defaultAdventurersDishName | 默认的冒险类料理名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:52 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.defaultAtkBoostingDishName | 默认的攻击类料理名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.defaultDefBoostingDishName | 默认的防御类料理名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:58 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.eatInterval | 吃药间隔时间（毫秒） 防止频繁吃药 | int | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.enabled | 是否启用自动吃药 | bool | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.showNotification | 是否显示吃药通知 | bool | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoEatConfig.testFoodName | 测试食物名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:40 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.actionSchedulerByCd | 根据技能CD优化出招人员 根据填入人或人和cd，来决定当此人元素战技cd未结束时，跳过此人出招，来优化战斗流程，可填入人名或人名数字（用逗号分隔）， 多种用分号分隔，例如:白术;钟离,12;，如果人名，则用内置cd检查，如果是人名和数字，则把数字当做出招cd(秒)。 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.battleThresholdForLoot | 拾取战斗人次阈值,当战斗人次小于一定次数，就结束战斗情况下，不触发拾取掉落物和万叶拾取后拾取，只有不小于2时才生效。 | int? | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:160 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.burstEnabled |  | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:181 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.damageNumberRecognitionMode | 伤害数字识别模式 | DamageNumberRecognitionMode | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:226 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.drawRecognitionResults | 绘制识别结果位置：在遮罩窗口上显示血条、伤害数字等识别结果的边框 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:232 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.enableCombatTargeting | 战斗中持续索敌：战斗过程中情况允许时持续尝试面朝敌人 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:208 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.expBasedPickupEnabled | 基于经验值判断是否执行战后拾取（检测到精英怪经验值图标时才拾取） | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:196 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.fightFinishDetectEnabled | 检测战斗结束 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.finishDetectConfig | 战斗结束相关配置 | FightFinishDetectConfig | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:142 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.guardianAvatar |  | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:172 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.guardianAvatarHold |  | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:178 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.guardianCombatSkip |  | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:175 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.kazuhaPartyName | 战斗结束后，如果不存在万叶，则切换至存在万叶的队伍（基于开启万叶拾取情况下） | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:187 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.kazuhaPickupEnabled | 战斗结束后，如果存在枫原万叶，则使用该角色捡材料 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:166 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.lockLostWaitTime | 脱锁等待时间（秒）：敌人不可见时等待一定时间后开始旋转索敌 | double | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:214 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.onlyPickEliteDropsMode | 只拾取精英掉落 Closed ：关闭功能 AllowAutoPickupForNonElite: 非精英允许自动拾取：战斗过程中掉落脚下的可以自动拾取，但不会执行万叶拾取和拾取配置逻辑。 DisableAutoPickupForNonElite: 非精英关闭拾取：战斗过程中掉落到脚下的也不会自动拾取。 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:40 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.pickDropsAfterFightEnabled | 战斗结束后光柱扫描掉落物 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:148 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.pickDropsAfterFightSeconds | 战斗结束后光柱扫描掉落物的持续秒数 | int | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:154 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.qinDoublePickUp |  | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:169 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.strategyName | 选择战斗策略；用于战斗 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:16 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.swimmingEnabled |  | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:190 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.targetingDetectionInterval | 索敌识别间隔（毫秒） | int | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:220 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.teamNames | 英文逗号分割 强制指定队伍角色 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFightConfig.timeout | 战斗超时，单位秒 | int | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:202 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFishingConfig.autoThrowRodEnabled | // 鱼儿上钩文字识别区域 // 暂时无用 // | bool | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:30 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFishingConfig.autoThrowRodTimeOut | 自动抛竿未上钩超时时间(秒) | int | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:35 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFishingConfig.enabled | 触发器是否启用 启用后： 1. 自动判断是否进入钓鱼状态 2. 自动提杆 3. 自动拉条 | bool | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFishingConfig.fishingTimePolicy | 昼夜策略 钓全天的鱼、还是只钓白天或夜晚的鱼 | FishingTimePolicy | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:47 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFishingConfig.wholeProcessTimeoutSeconds | 整个任务超时时间 | int | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:40 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoFixWin11BitBlt | 自动修复Win11下BitBlt截图方式不可用的问题 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:89 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoGeniusInvokationConfig.activeCharacterCardSpace | // 角色卡牌区域向左扩展距离，包含HP区域 // | int | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:44 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoGeniusInvokationConfig.characterCardExtendHpRect | HP区域 在 角色卡牌区域 的相对位置 | Rect | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoGeniusInvokationConfig.defaultCharacterCardRects |  | List<Rect> | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:18 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoGeniusInvokationConfig.myDiceCountRect | 骰子数量文字识别区域 | Rect | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:29 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoGeniusInvokationConfig.sleepDelay | 设置延时（毫秒）；如果频繁出现操作速度过快，操作动画未播放完毕的情况可以添加延时 | int | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:16 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoGeniusInvokationConfig.strategyName | 选择卡组；选择你想要使用的卡组与策略 | string | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:14 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.count | 刷取次数；树脂耗尽模式关闭或统计失败时使用的固定次数。 | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.country | 国家；按国家选择刷取对应的地脉花。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:18 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.actionSchedulerByCd | 根据技能CD优化出招人员。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:25 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.burstEnabled |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.fightFinishDetectEnabled | 检测战斗结束。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.finishDetectConfig |  | FightFinishDetectConfig | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.guardianAvatar |  | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.guardianAvatarHold |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:48 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.guardianCombatSkip |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:47 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.kazuhaPickupEnabled |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.qinDoublePickUp |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:52 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.seekEnemyEnabled |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:54 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.seekEnemyIntervalSeconds |  | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:55 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.seekEnemyRotaryFactor |  | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:56 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.strategyName |  | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:10 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.swimmingEnabled |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:50 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.teamNames | 英文逗号分割，强制指定队伍角色。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.fightConfig.timeout |  | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:53 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.friendshipTeam | 好感队名称；领取奖励前切换到该队伍，留空则不切换。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.isGoToSynthesizer |  | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.isResinExhaustionMode | 树脂耗尽模式；按当前树脂与库存自动计算可刷次数，结束后自动停止。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.leyLineOutcropType | 地脉花类型；选择刷取的地脉花，启示之花（经验书）或藏金之花（摩拉）。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.openModeCountMin | 刷取次数取小值；与手动次数取最小值，避免超过树脂可用次数。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:24 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.scanDropsAfterRewardEnabled | 是否在领取地脉花奖励后扫描周围掉落物光柱。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.scanDropsAfterRewardSeconds | 领取奖励后扫描掉落物的最长时长，单位为秒。 | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:57 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.team | 战斗队伍名称 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:36 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.timeout |  | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:42 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.useFragileResin | 使用脆弱树脂；原粹与浓缩耗尽后，允许使用脆弱树脂继续刷取。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoLeyLineOutcropConfig.useTransientResin | 使用须臾树脂；原粹与浓缩耗尽后，允许使用须臾树脂继续刷取。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:30 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoMusicGameConfig.musicLevel | 乐曲级别 | string | BetterGenshinImpact/GameTask/AutoMusicGame/AutoMusicGameConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoMusicGameConfig.mustCanorusLevel | 自动达到大音天籁的级别 | bool | BetterGenshinImpact/GameTask/AutoMusicGame/AutoMusicGameConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.blacklistModePickEnabled | 黑名单模式的拾取规则启用状态 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:67 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.enabled | 触发器是否启用 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:22 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.fastModeEnabled | 急速模式 无视文字识别结果，直接拾取 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:52 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.itemIconLeftOffset | 1080p下拾取文字左边的起始偏移 | int | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.itemTextLeftOffset | 1080p下拾取文字的起始偏移 | int | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:32 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.itemTextRightOffset | 1080p下拾取文字的终止偏移 | int | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:37 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.mode | 自动拾取名单模式 | AutoPickMode | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:62 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.ocrEngine | 文字识别引擎 - Paddle - Yap | string | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:44 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.pickKey | 自定义按键拾取 | string | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:57 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoPickConfig.whitelistModeDoNotPickEnabled | 白名单模式的不拾取规则启用状态 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:71 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoRedeemCodeConfig.clipboardListenerEnabled | 是否启用剪切板监听 | bool | BetterGenshinImpact/GameTask/UseRedeemCode/AutoRedeemCodeConfig.cs:12 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.afterChooseOptionSleepDelay | 选择选项前的延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:36 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoGetDailyRewardsEnabled | 自动领取每日委托奖励 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:66 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoHangoutChooseOptionSleepDelay | 自动邀约选择选项前的延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:117 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoHangoutEndChoose | 自动邀约分支选择 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:111 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoHangoutEventEnabled | 自动邀约启用 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:105 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoHangoutPressSkipEnabled | 自动邀约自动点击跳过按钮 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:123 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoReExploreCharacter | 自动重新派遣使用角色配置，逗号分割 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:78 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoReExploreEnabled | 自动重新派遣 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:72 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.autoWaitDialogueOptionVoiceEnabled | 选择剧情选项前，通过游戏进程音频的人声检测自动等待语音结束 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:42 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.beforeClickConfirmDelay | 点击对话框前的延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:60 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.bringGameToFrontAfterBackgroundDialogEnabled | 后台剧情结束后切回游戏前台 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:149 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.clickChatOption | 优先选择第一个选项 优先选择最后一个选项 不选择选项 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:87 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.closePopupPagedEnabled | 关闭弹出层 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:175 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.customPriorityOptions | 自定义优先选项文本，每行一个或用分号分隔 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:99 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.customPriorityOptionsEnabled | 自定义优先选项启用 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:93 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.dialogueOptionVoiceMaxWaitSeconds | 人声检测等待语音结束的最大等待时间（秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:48 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.dialogueOptionVoiceVadDiagnosticEnabled | 持续检测游戏进程音频并在设置页显示 Silero VAD 诊断数据 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:54 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.enabled | 触发器是否启用 启用后： 1. 快速跳过对话 2. 自动点击一个识别到的选项 3. 黑屏过长自动点击跳过 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.pictureInPictureEnabled | 游戏失焦时显示画中画 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:161 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.pictureInPictureSourceType | 画中画的源图像类型 TriggerDispatcher：来自于截图器50ms一次 CaptureLoop：主动获取（60帧） | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:169 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.quicklySkipConversationsEnabled | 快速跳过对话 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.runBackgroundEnabled | 后台运行 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:143 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.skipBuiltInClickOptions | JS调用时跳过内置默认点击选项 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:181 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoSkipConfig.submitGoodsEnabled | 提交物品 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:155 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.autoArtifactSalvage | 结束后是否自动分解圣遗物 | bool | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:18 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.bossNum | boss 名字 1~3 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:14 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.condensedResinUseCount | 使用浓缩树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:38 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.fightTeamName | 指定战斗队伍 | string | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:50 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.fragileResinUseCount | 使用脆弱树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.originalResinUseCount | 使用原粹树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.resinPriorityList | 自定义使用树脂优先级 | List<string> | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.specifyResinUse | 指定树脂的使用次数 | bool | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:22 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.strategyName | 选择战斗策略；用于战斗 | string | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:10 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoStygianOnslaughtConfig.transientResinUseCount | 使用须臾树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:42 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoWoodConfig.afterZSleepDelay | 使用小道具后的额外延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoWood/AutoWoodConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoWoodConfig.useWonderlandRefresh | 使用进出千星奇域刷新CD | bool | BetterGenshinImpact/GameTask/AutoWood/AutoWoodConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| autoWoodConfig.woodCountOcrEnabled | 木材数量OCR是否启用 | bool | BetterGenshinImpact/GameTask/AutoWood/AutoWoodConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| captureMode | 窗口捕获的方式 | string | BetterGenshinImpact/Core/Config/AllConfig.cs:43 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.audioMuted | 桌面分身的 RDP 音频是否静音，不影响主桌面的其他程序。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:54 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.gameMouseModeEnabled | 桌面分身是否启用游戏鼠标模式。默认使用普通鼠标模式。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:48 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.keepAspectRatio | 桌面分身窗口是否保持 16:9 宽高比。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:36 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.normalWindowPosition.left |  | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:61 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.normalWindowPosition.top |  | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:63 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.sendSystemShortcutsToRemote | Alt+Tab、Windows 键等系统组合键是否发送到桌面分身。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:42 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.smallWindowPosition.left |  | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:61 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.smallWindowPosition.top |  | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:63 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.smartSizingEnabled | RDP 画面是否自适应窗口。关闭时使用 1:1 显示。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:30 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| childSessionConfig.topmostEnabled | 桌面分身窗口是否置顶。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:24 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.currentBackdropType | 主题（旧版主题，兼容性保留） | WindowBackdropType | BetterGenshinImpact/Core/Config/CommonConfig.cs:63 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.currentThemeType | 当前主题类型（新版主题） | ThemeType | BetterGenshinImpact/Core/Config/CommonConfig.cs:57 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.exitToTray | 退出时最小化至托盘 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.isFirstRun | 是否是第一次运行 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:93 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.mainBackgroundEnabled | 是否启用主窗口自定义背景图 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:69 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.mainBackgroundImagePath | 主窗口自定义背景图路径，空字符串表示未设置 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:75 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.mainBackgroundOpacity | 主窗口背景图不透明度，数值越小背景越淡，文字越清晰 | double | BetterGenshinImpact/Core/Config/CommonConfig.cs:81 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.mainBackgroundStretch | 主窗口背景图拉伸模式 | Stretch | BetterGenshinImpact/Core/Config/CommonConfig.cs:87 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.onceHadRunDeviceIdList | 一个设备只运行一次的已运行设备ID列表 | List<string> | BetterGenshinImpact/Core/Config/CommonConfig.cs:105 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.redeemCodeCnFeedsNotificationEnabled | 国服兑换码更新是否允许通知 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:118 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.redeemCodeFeedsUpdateVersion | 当前看过的兑换码推送版本 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:112 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.redeemCodeGlobalFeedsNotificationEnabled | 国际服兑换码更新是否允许通知 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:124 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.redeemCodeGlobalFeedsUpdateVersion | 当前看过的国际服兑换码推送版本 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:130 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.rewardRecognitionScreenshotEnabled | 是否保存奖励识别调试截图 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.runForVersion | 这个版本是否运行过 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:99 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.screenshotEnabled | 是否启用遮罩窗口 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| commonConfig.screenshotUidCoverEnabled | UID遮盖是否启用 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| detailedErrorLogs | 详细的错误日志 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| devConfig.recognitionAssetsRootPath | Recognition 模板制作工具最近使用的 Assets 根目录 | string | BetterGenshinImpact/Core/Config/DevConfig.cs:29 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| devConfig.recognitionAssetsRootPathHistory | Recognition 模板制作工具最近使用的输出文件夹历史，按最近使用顺序保存，最多 10 条。 | List<string> | BetterGenshinImpact/Core/Config/DevConfig.cs:35 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| devConfig.recognitionJsonPath | Recognition 模板制作工具最近使用的配置文件 | string | BetterGenshinImpact/Core/Config/DevConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| devConfig.recognitionJsonPathHistory | Recognition 模板制作工具最近使用的配置文件历史，按最近使用顺序保存，最多 10 条。 | List<string> | BetterGenshinImpact/Core/Config/DevConfig.cs:25 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| devConfig.recordMapName | 录制地图名称 | string | BetterGenshinImpact/Core/Config/DevConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| disableInputMonitor | 禁用键鼠监听，需重启 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:104 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.autoDisableGenshinHdrEnabled | 启动前自动关闭原神 HDR（删除原神 HDR 对应注册表键） | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:54 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.autoEnterGameEnabled | 自动进入游戏（开门） | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.genshinStartArgs | 原神启动参数 | string | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.installPath | 原神安装路径 | string | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.linkedStartEnabled | 联动启动原神本体 | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.recordGameTimeEnabled | 使用Starward同步记录时间 | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| genshinStartConfig.startGameWithCmd | 使用 CMD 启动游戏；如果原神弹窗“检测到非法工具，请重启机器”，请尝试开启此选项 | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:48 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| getGridIconsConfig.gridName | Grid界面名称 | GridScreenName | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:13 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| getGridIconsConfig.lvAsSuffix | 使用等级作为后缀 | bool | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| getGridIconsConfig.maxNumToGet | 最多获取多少个图标 | int | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:25 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| getGridIconsConfig.starAsSuffix | 使用星星作为后缀 | bool | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:17 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.additionalPath | 附加path，用;分割。默认为空。 | string | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.autoAppendCudaPath | 自动附加cuda的path。一般情况下用这个就足够了。默认关闭。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:55 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.cpuOcr | 是否强制OCR使用CPU推理。在某些环境上使用GPU进行OCR推理会导致性能下降(比如很多使用DirectML推理的情况下)。默认开启。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.cudaDevice | 强制指定cuda设备,默认为0(使用默认设备) | int | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.embedTensorRtCache | 嵌入式引擎缓存。将引擎缓存嵌入到模型中。默认开启。关闭它可能会提高性能(如果不爆炸的话)。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:71 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.enableOpenVinoCache | 启用 OpenVINO 缓存。默认关闭。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:86 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.enableTensorRtCache | 启用TensorRT缓存。默认开启。不开的话使用TensorRT每次加载模型会卡爆。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:65 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.gpuDevice | 强制指定gpu设备,默认为0(使用默认设备) | int | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.inferenceDevice | 推理使用的设备。默认CPU | InferenceDeviceType | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:13 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.openVinoDevice | OpenVino 设备参数。 | string | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:80 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hardwareAccelerationConfig.optimizedModel | 是否输出优化后的模型文件到缓存。注意:在不支持的执行器上使用会导致异常。默认关闭。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.addWaypointHotkey | 添加路径记录点 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:222 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.addWaypointHotkeyType | 「添加路径点」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:225 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoCookGameHotkey | 自动烹饪开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:180 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoCookGameHotkeyType | 「启动/停止自动烹饪」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:183 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoDomainHotkey | 触发「启动/停止自动秘境」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoDomainHotkeyType | 「启动/停止自动秘境」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:22 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoFightHotkey | 触发「启动/停止自动战斗」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:25 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoFightHotkeyType | 「启动/停止自动战斗」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:28 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoFishingEnabledHotkey | 触发「自动钓鱼开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:31 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoFishingEnabledHotkeyType | 「自动钓鱼开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoFishingGameHotkey | 活动音游开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:173 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoFishingGameHotkeyType | 「启动/停止自动钓鱼」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:176 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoGeniusInvokationHotkey | 触发「启动/停止自动七圣召唤」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:37 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoGeniusInvokationHotkeyType | 「启动/停止自动七圣召唤」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:40 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoMusicGameHotkey | 活动音游开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:166 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoMusicGameHotkeyType | 「启动/停止自动音游」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:169 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoPickEnabledHotkey | 触发「自动拾取开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:43 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoPickEnabledHotkeyType | 「自动拾取开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoSkipEnabledHotkey | 触发「自动剧情开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoSkipEnabledHotkeyType | 「自动剧情开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:52 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoSkipHangoutEnabledHotkey | 触发「自动邀约开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:55 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoSkipHangoutEnabledHotkeyType | 「自动邀约开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:58 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoTrackHotkey |  | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:13 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoTrackHotkeyType |  | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:16 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoTrackPathHotkey | 自动寻路 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:187 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoTrackPathHotkeyType |  | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:190 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoWoodHotkey | 触发「启动/停止自动伐木」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:61 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.autoWoodHotkeyType | 「启动/停止自动伐木」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:64 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.bgiEnabledHotkey | 触发「启动停止 BetterGI」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:67 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.bgiEnabledHotkeyType | 「启动停止 BetterGI」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:70 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.cancelTaskHotkey | 停止任意独立任务 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:262 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.cancelTaskHotkeyType | 「停止当前脚本/独立任务」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:265 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.clickGenshinCancelButtonHotkey | 点击取消按钮 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:145 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.clickGenshinCancelButtonHotkeyType | 「快捷点击原神内取消按钮」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:148 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.clickGenshinConfirmButtonHotkey | 点击确认按钮 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:138 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.clickGenshinConfirmButtonHotkeyType | 「快捷点击原神内确认按钮」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:141 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.enhanceArtifactHotkey | 触发「按下快速强化圣遗物」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:73 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.enhanceArtifactHotkeyType | 「按下快速强化圣遗物」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:76 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.executePathHotkey | 路径执行 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:229 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.executePathHotkeyType | 「（测试）播放内存中的路径」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:232 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.keyMouseMacroRecordHotkey | 键鼠录制/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:250 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.keyMouseMacroRecordHotkeyType | 「启动/停止键鼠录制」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:253 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.logBoxDisplayHotkey | 日志与状态窗口展示 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:236 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.logBoxDisplayHotkeyType | 「日志与状态窗口展示开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:239 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.mapMaskEnabledHotkey | 地图遮罩开关 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:276 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.mapMaskEnabledHotkeyType | 「地图遮罩开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:279 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.mapPosRecordHotkey | 地图路线录制开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:159 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.mapPosRecordHotkeyType |  | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:162 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.oneKeyClaimRewardHotkey | 触发「一键领取奖励」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:85 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.oneKeyClaimRewardHotkeyType | 「一键领取奖励」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:88 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.oneKeyFightHotkey | 一键战斗宏 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:152 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.oneKeyFightHotkeyType | 「一键战斗宏快捷键」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:155 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.onedragonHotkey | 停止任意独立任务 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:269 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.onedragonHotkeyType | 「启动/停止一条龙」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:272 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.overlayMetricsDisplayHotkey | 遮罩指标栏展示 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:243 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.overlayMetricsDisplayHotkeyType | 「遮罩指标栏展示开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:246 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.pathRecorderHotkey | 路径记录开始 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:215 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.pathRecorderHotkeyType | 「启动/停止路径记录器」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:218 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickBuyHotkey | 触发「按下快速购买商店物品」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:79 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickBuyHotkeyType | 「按下快速购买商店物品」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:82 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickSereniteaPotHotkey | 触发「按下快速进出尘歌壶」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:91 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickSereniteaPotHotkeyType | 「按下快速进出尘歌壶」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:94 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickTeleportEnabledHotkey | 触发「快速传送开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:97 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickTeleportEnabledHotkeyType | 「快速传送开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:100 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickTeleportTickHotkey | 快捷传送触发 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:104 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.quickTeleportTickHotkeyType | 「手动触发快速传送触发快捷键（按住起效）」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:107 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.recBigMapPosHotkey | 路径记录开始 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:208 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.recBigMapPosHotkeyType | 「（开发）获取当前大地图中心点位置」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:211 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.recognitionTemplateEditorHotkey | Recognition 模板素材制作 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:125 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.recognitionTemplateEditorHotkeyType | 「（开发）模板素材制作」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:128 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.skillCdEnabledHotkey | 技能CD提示开关 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:111 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.skillCdEnabledHotkeyType | 「冷却提示开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:114 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.suspendHotkey | 暂停 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:256 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.suspendHotkeyType | 「暂停当前脚本/独立任务」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:259 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.takeScreenshotHotkey | 截图 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:118 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.takeScreenshotHotkeyType | 「游戏截图」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:121 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.test1Hotkey | 测试 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:194 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.test1HotkeyType | 「（测试）测试」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:197 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.test2Hotkey | 测试2 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:201 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.test2HotkeyType | 「（测试）测试2」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:204 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.turnAroundHotkey | 触发「长按旋转视角 - 那维莱特转圈」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:131 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| hotKeyConfig.turnAroundHotkeyType | 「长按旋转视角 - 那维莱特转圈」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:134 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.abandonChallenge | 中断挑战 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:130 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.checkTutorialDetails | 查看教程详情 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:272 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.drop | 落下 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:100 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.elementalBurst | 元素爆发 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:70 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.elementalSight | 长按打开元素视野 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:278 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.elementalSkill | 元素战技 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:64 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.globalKeyMappingEnabled | 是否启用全局按键映射 | bool | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.hideUI | 隐藏主界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:302 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.interactionInSomeMode | 特定玩法内交互操作 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:118 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.jump | 跳跃；特定操作模式下向上移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:94 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.moveBackward | 向后移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.moveForward | 向前移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:28 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.moveLeft | 向左移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:40 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.moveRight | 向右移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.normalAttack | 普通攻击 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:58 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openAdventurerHandbook | 打开冒险之证界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:200 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openBattlePassScreen | 打开纪行界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:218 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openCharacterScreen | 打开角色界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:182 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openChatScreen | 打开聊天界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:260 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openCoOpScreen | 打开多人游戏界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:206 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openFriendsScreen | 打开好友界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:296 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openInventory | 打开背包 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:176 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openMap | 打开地图 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:188 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openNotificationDetails | 打开通知详情 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:254 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openPaimonMenu | 打开派蒙界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:194 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openPartySetupScreen | 打开队伍配置界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:290 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openQuestMenu | 开关任务菜单 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:248 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openSpecialEnvironmentInformation | 打开特殊环境说明 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:266 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openStellarReunion | 打开星之归还（条件符合期间生效） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:242 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openTheEventsMenu | 打开活动面板 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:224 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openTheFurnishingScreen | 打开摆设界面（尘歌壶内） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:236 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openTheSettingsMenu | 打开玩法系统界面（尘歌壶内猫尾酒馆内） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:230 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.openWishScreen | 打开祈愿界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:212 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.pickUpOrInteract | 拾取/交互（自动拾取由AutoPick模块管理） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:106 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.questNavigation | 开启任务追踪 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:124 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.quickUseGadget | 快捷使用小道具 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:112 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.shortcutWheel | 呼出快捷轮盘 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:166 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.showCursor | 呼出鼠标 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:284 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.sprintKeyboard | 冲刺（键盘） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:76 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.sprintMouse | 冲刺（鼠标） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:82 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchAimingMode | 切换瞄准模式 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:88 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchMember1 | 切换小队角色1 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:136 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchMember2 | 切换小队角色2 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:142 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchMember3 | 切换小队角色3 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:148 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchMember4 | 切换小队角色4 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:154 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchMember5 | 切换小队角色5 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:160 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| keyBindingsConfig.switchToWalkOrRun | 切换走/跑；特定操作模式下向下移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:52 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.combatMacroEnabled | 一键战斗宏启用状态 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:57 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.combatMacroHotkeyMode | 一键战斗宏快捷键模式 | string | BetterGenshinImpact/Core/Config/MacroConfig.cs:63 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.combatMacroPriority | 一键战斗宏优先级 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:69 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.enhanceWaitDelay | 高延迟下强化的额外等待时间 https://github.com/babalae/better-genshin-impact/issues/9 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.fFireInterval | F连发时间间隔 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.fPressHoldToContinuationEnabled | 长按F变F连发 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.oneKeyClaimRewardHotkeyMode | 一键领取奖励快捷键模式 | string | BetterGenshinImpact/Core/Config/MacroConfig.cs:75 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.oneKeyClaimRewardScrollDownAmount | 一键领取奖励滚轮下滑幅度 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:87 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.oneKeyClaimRewardScrollDownEnabled | 一键领取奖励未找到领取图标时滚轮下滑 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:81 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.runaroundInterval | 转圈圈时间间隔 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.runaroundMouseXInterval | 转圈圈鼠标右移长度 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.spaceFireInterval | 空格连发时间间隔 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| macroConfig.spacePressHoldToContinuationEnabled | 长按空格变空格连发 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| mapMaskConfig.enabled | 是否启用 | bool | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| mapMaskConfig.hoYoLabLanguage |  | string | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| mapMaskConfig.mapPointApiProvider |  | MapPointApiProvider | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| mapMaskConfig.miniMapMaskEnabled | 小地图遮罩是否启用 | bool | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| mapMaskConfig.pathAutoRecordEnabled | 自动记录路径功能是否启用 | bool | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:32 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.crosshairColor | 准星颜色（十六进制） | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:115 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.crosshairEnabled | 准星是否启用 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:103 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.crosshairGap | 中心点与十字线的间隔（仅 DotCrosshair 类型） | int | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:133 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| maskWindowConfig.crosshairImagePath | 自定义准星图片路径 | string? | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:166 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.crosshairLineWidth | 准星线宽 | int | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:121 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| maskWindowConfig.crosshairScaleMode | 自定义图片缩放方式 | CrosshairScaleMode | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:172 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.crosshairSize | 准星大小 | int | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:127 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| maskWindowConfig.crosshairType | 准星类型 | CrosshairType | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:109 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.customHtmlMaskAutoReloadOnSave | 保存后自动刷新 HTML：开启后保存 HTML 会立即刷新已经打开的自定义遮罩。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:430 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.customHtmlMaskClickThrough | HTML 遮罩鼠标穿透：开启后鼠标可穿透遮罩；关闭后 HTML 内容可以接收点击和输入。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:427 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.customHtmlMaskEnabled |  | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:424 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionFontSize | 方位文字大小：小地图方位文字字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:388 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionShadowBlurRadius | 方位文字阴影模糊半径：方位文字阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:400 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionShadowColor | 方位文字阴影颜色：方位文字阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:394 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionShadowEnabled | 显示方位文字阴影：开启后方位文字在复杂背景上更容易看清。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:391 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionShadowOpacity | 方位文字阴影透明度：方位文字阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:397 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionTextColor | 方位文字颜色：小地图周围方位文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:385 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.directionsEnabled | 方位提示是否启用 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:56 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.displayRecognitionResultsOnMask | 是否在遮罩窗口上显示识别结果 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:62 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logFontFamily |  | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:298 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logFontScale | 遮罩 UI 缩放率 (0.5-3.0)，叠加到日志、状态和 FPS 的基础字号上。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:205 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| maskWindowConfig.logFontSize | 日志文字大小：日志内容字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:301 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logPanelBackgroundColor | 日志区域背景色：日志窗口底色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:286 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logPanelBorderColor | 日志区域边框颜色：日志窗口边框颜色。边框粗细为 0 时不会显示边框。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:289 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logPanelBorderThickness | 日志区域边框粗细：日志窗口边框线宽。填 0 表示不显示边框。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:292 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logShadowBlurRadius | 日志阴影模糊半径：日志阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:313 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logShadowColor | 日志阴影颜色：日志阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:307 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logShadowEnabled | 显示日志阴影：开启后文字和区域更容易从游戏背景中区分出来。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:304 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logShadowOpacity | 日志阴影透明度：日志阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:310 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logTextBoxHeightRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:445 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logTextBoxLeftRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:436 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logTextBoxTopRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:439 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logTextBoxWidthRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:442 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.logTextColor | 日志文字颜色：日志内容的文字颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:295 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.maskEnabled | 是否启用遮罩窗口 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:68 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsFontFamily |  | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:358 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsFontScale | 指标栏缩放率 (0.5-3.0)，叠加到指标栏的基础字号和布局尺寸上。 独立于遮罩 UI 缩放率。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:212 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| maskWindowConfig.metricsFontSize | 指标文字大小：指标栏字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:361 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsHeightRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:469 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsItemWidth | 单个指标项宽度：每个指标项占用的宽度。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:367 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsLeftRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:460 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsLineHeight | 指标单行高度：每一行指标占用的高度。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:364 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsNameColumnWidth | 指标名称列宽度：指标名称这一列的宽度。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:370 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsPanelBackgroundColor | 指标栏背景色：指标栏底色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:346 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsPanelBorderColor | 指标栏边框颜色：指标栏边框颜色。边框粗细为 0 时不会显示边框。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:349 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsPanelBorderThickness | 指标栏边框粗细：指标栏边框线宽。填 0 表示不显示边框。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:352 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsShadowBlurRadius | 指标栏阴影模糊半径：指标栏阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:382 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsShadowColor | 指标栏阴影颜色：指标栏阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:376 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsShadowEnabled | 显示指标栏阴影：开启后指标栏在复杂背景上更容易看清。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:373 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsShadowOpacity | 指标栏阴影透明度：指标栏阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:379 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsTextColor | 指标文字颜色：指标文字颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:355 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsTopRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:463 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.metricsWidthRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:466 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.overlayLayoutEditEnabled | 启用拖拽调整位置大小；开启后可以拖拽调整日志、状态栏与指标栏位置，并调整大小 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:433 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.overlayMetricItems | 配置文件里使用 string key 便于兼容旧版本，读取后由 EnsureOverlayMetricItems 约束回固定枚举集合。 | Dictionary<string, bool> | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:188 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.overlayScalingEnabled | 是否启用遮罩 UI 缩放。关闭后直接使用各遮罩元素的基础尺寸。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:199 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.overlayWindowBackgroundColor | 主遮罩窗口背景色：遮罩窗口本身的背景色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:280 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionLineStrokeColor | 识别线条颜色：开启统一颜色后，识别线条使用的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:412 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionLineStrokeThickness | 识别线条线宽：开启统一颜色后，识别线条的线宽。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:415 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionRectStrokeColor | 识别矩形边框颜色：开启统一颜色后，识别矩形框使用的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:406 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionRectStrokeThickness | 识别矩形线宽：开启统一颜色后，识别矩形框的线宽。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:409 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionTextColor | 识别文字颜色：普通识别结果文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:418 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionTextFontSize | 识别文字大小：普通识别结果文字的基础字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:421 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.recognitionUseDrawableStyle | 统一识别框线颜色：关闭时保留任务自己指定的颜色；开启后使用下面设置的统一颜色。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:403 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.showFps | 显示FPS | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:178 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.showLogBox | // 显示遮罩窗口边框 // | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:79 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.showOverlayMetrics | 显示遮罩指标栏 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:184 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.showStatus | 显示状态指示 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:85 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusDisabledTextColor | 未启用状态文字颜色：任务未启用时图标和文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:325 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusEnabledTextColor | 已启用状态文字颜色：任务启用时图标和文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:328 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusFontSize | 状态文字大小：状态栏字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:331 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusListHeightRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:457 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusListLeftRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:448 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusListTopRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:451 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusListWidthRatio |  | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:454 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusPanelBackgroundColor | 任务状态栏背景色：状态栏底色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:316 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusPanelBorderColor | 任务状态栏边框颜色：状态栏边框颜色。边框粗细为 0 时不会显示边框。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:319 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusPanelBorderThickness | 任务状态栏边框粗细：状态栏边框线宽。填 0 表示不显示边框。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:322 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusShadowBlurRadius | 状态栏阴影模糊半径：状态栏阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:343 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusShadowColor | 状态栏阴影颜色：状态栏阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:337 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusShadowEnabled | 显示状态栏阴影：开启后状态栏在复杂背景上更容易看清。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:334 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.statusShadowOpacity | 状态栏阴影透明度：状态栏阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:340 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.textOpacity | 遮罩文本透明度 (0.0-1.0) | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:193 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.uidCoverEnabled | UID遮盖是否启用 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:91 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| maskWindowConfig.wineOverlayBackgroundColor | Wine 兼容背景色：仅在 Wine 环境下使用的兼容背景色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:283 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.autoSwitchInstrument | 演奏前是否自动切换到当前曲目的输出乐器 | bool | BetterGenshinImpact/Core/Config/MusicConfig.cs:48 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.customBpm | 自定义 BPM | double | BetterGenshinImpact/Core/Config/MusicConfig.cs:42 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.inputMode | 默认使用后台 PostMessage 演奏 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:18 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.musicFolder | 曲谱扫描根目录 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:12 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.playbackMode | 播放模式 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:24 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.selectedInstrumentProfile | 默认输出乐器档案 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:54 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.speed | 播放速度 | double | BetterGenshinImpact/Core/Config/MusicConfig.cs:30 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| musicConfig.useCustomBpm | 是否使用自定义 BPM 覆盖曲谱的基准速度 | bool | BetterGenshinImpact/Core/Config/MusicConfig.cs:36 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| nextScheduledTask | /// <summary> /// 推理使用的设备 /// </summary> [ObservableProperty] private string _inferenceDevice = "CPU"; | List<ValueTuple<string, int, string, string>> | BetterGenshinImpact/Core/Config/AllConfig.cs:98 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notShowNewVersionNoticeEndVersion | 不展示新版本提示的最新版本 | string | BetterGenshinImpact/Core/Config/AllConfig.cs:55 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkAction | 传"none"时，点击推送不会弹窗 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkApiEndpoint | Bark API 端点；填写 Bark API 端点，例如：api.day.app | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:22 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkAutoCopy | iOS14.5以下自动复制推送内容，1为开启 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkBadge | 推送角标，可以是任意数字 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:32 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkCall | 通知铃声重复播放，1为开启 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:37 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkCiphertext | 加密密钥；推送内容加密密钥 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkCopy | 复制推送时指定复制的内容 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:44 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkDeviceKeys | 设备 Key；多个设备使用英文逗号、分号或空格分隔 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkGroup | 对消息进行分组，推送将按group分组显示在通知中心中 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkIcon | 为推送设置自定义图标URL | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:56 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkIsArchive | 传1保存推送，传其他的不保存推送 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:61 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkLevel | 推送中断级别：critical(重要警告), active(默认值), timeSensitive(时效性通知), passive(仅添加到通知列表) | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:66 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkNotificationEnabled | Bark移动推送通知配置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:71 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkSound | 通知声音 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:76 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkSubtitle | 推送副标题 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:83 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkUrl | 点击推送时跳转的URL | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:88 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.barkVolume | 重要警告的通知音量，取值范围: 0-10 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:93 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.dingDingSecret | 钉钉Webhook密钥 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:98 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.dingDingwebhookNotificationEnabled | dindin 通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:103 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.dingdingWebhookUrl | 钉钉Webhook地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:108 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.discordWebhookAvatarUrl | Discord Webhook头像地址 Default url from https://www.bettergi.com/ | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:278 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.discordWebhookImageEncoder | 截图编码；PNG 无损，JPEG 快速压缩，WebP 档案小，按需选择 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:283 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.discordWebhookNotificationEnabled | Discord Webhook推送通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:262 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.discordWebhookUrl | Discord Webhook地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:267 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.discordWebhookUsername | Discord Webhook用户名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:272 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.dragonEndSummaryEnabled | 一条龙结束后发送汇总通知（体力+委托奖励，附拼接截图） | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:113 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.emailNotificationEnabled | Email 通知配置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:116 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.feishuAppId | 飞书AppId；若填写AppId、AppSecret则发送图片 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:152 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.feishuAppSecret | 飞书AppSecret；若填写AppId、AppSecret则发送图片 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:153 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.feishuNotificationEnabled | 飞书通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:144 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.feishuWebhookUrl | 飞书通知地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:150 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.fromEmail | 发件人邮箱；填写发件人邮箱 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:118 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.fromName | 发件人姓名；填写发件人姓名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:120 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.gotifyAppToken | Gotify服务APP Token | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:323 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.gotifyNotificationEnabled | Gotify通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:313 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.gotifyNotifyLevel | Gotify通知优先级 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:328 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.gotifyUrl | Gotify服务地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:318 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.includeScreenShot | 是否包含截图 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:126 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.jsNotificationEnabled | 是否允许 js 发送通知 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.meowNickname | MeoW用户昵称 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:303 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.meowNotificationEnabled | MeoW通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:298 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.meowTitle | MeoW消息标题（可选，路径参数） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:308 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.notificationEventSubscribe |  | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:129 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.oneBotEndpoint | OneBot通知地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:166 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.oneBotGroupId | 群号；填写接收消息的群号 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:169 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.oneBotNotificationEnabled | OneBot通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:160 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.oneBotToken | Token；填写 OneBot Token（可选） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:170 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.oneBotUserId | QQ 号；填写接收消息的 QQ 号 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:168 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.qqAppId | QQ开放平台 AppID | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:338 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.qqClientSecret | QQ开放平台 AppSecret | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:343 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.qqGroupOpenId | 群聊 OpenID（群聊场景） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:353 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.qqNotificationEnabled | QQ通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:333 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.qqOpenId | 用户的 C2C OpenID（单聊场景） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:348 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.serverChanNotificationEnabled | ServerChan通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:288 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.serverChanSendKey | ServerChan SendKey | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:293 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.smtpPassword | SMTP 密码；填写 SMTP 密码 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:131 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.smtpPort | SMTP 服务器端口；填写 SMTP 服务器端口，一般为：587 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:133 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.smtpServer | SMTP 服务器；填写 SMTP 服务器 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:135 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.smtpUsername | SMTP 用户名；填写 SMTP 用户名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:137 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.telegramApiBaseUrl | Telegram API基础URL(可选，留空使用官方API) | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:175 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.telegramBotToken | Telegram机器人Token | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:190 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.telegramChatId | Telegram聊天ID | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:195 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.telegramNotificationEnabled | Telegram通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:201 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.telegramProxyEnabled | 是否启用Telegram代理 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:185 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.telegramProxyUrl | Telegram代理地址(可选，格式：http://127.0.0.1:7890) | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:180 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.toEmail | 收件人邮箱；填写收件人邮箱 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:203 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.webSocketEndpoint | WebSocket 端点；填写 WebSocket 端点 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:216 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.webSocketNotificationEnabled | 启用 WebSocket；WebSocket 相关设置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:218 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.webhookEnabled | 启用 Webhook；Webhook 相关设置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:207 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.webhookEndpoint | Webhook 端点；填写 Webhook 端点 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:211 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.webhookSendTo | 修改属性名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:214 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.wechatClawbotBaseUrl | 微信 Clawbot API 基础地址（登录响应 baseurl；为空时使用默认 https://ilinkai.weixin.qq.com） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:373 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.wechatClawbotBotToken | 微信 Clawbot 扫码登录获得的 bot_token | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:363 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.wechatClawbotNotificationEnabled | 微信 Clawbot 通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:358 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.wechatClawbotToUserId | 微信 Clawbot 推送目标用户 ID（用户给机器人发消息后自动获取） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:368 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.windowsUwpNotificationEnabled | windows uwp 通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:223 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.workweixinNotificationEnabled | 企业微信通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:230 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.workweixinWebhookUrl | 企业微信通知通知地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:236 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.xxtuiApiKey | xx信息推送通知API密钥 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:242 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.xxtuiChannels | xx信息推送通知渠道（WX_MP,WX_QY_ROBOT,DING_ROBOT,BARK） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:247 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.xxtuiFrom | xx信息推送通知来源 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:252 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| notificationConfig.xxtuiNotificationEnabled | 信息推送通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:257 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.autoFetchDispatchAdventurersGuildCountry | 自动领取派遣任务城市 | string | BetterGenshinImpact/Core/Config/OtherConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.autoRestartConfig |  | AutoRestart | BetterGenshinImpact/Core/Config/OtherConfig.cs:28 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.farmingPlanConfig | 锄地规划 | FarmingPlan | BetterGenshinImpact/Core/Config/OtherConfig.cs:31 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.gameCultureInfoName | 游戏语言名称 | string | BetterGenshinImpact/Core/Config/OtherConfig.cs:123 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.itemIconRecognitionMode | 物品图标识别模型 | ItemIconRecognitionMode | BetterGenshinImpact/Core/Config/OtherConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.miyousheConfig |  | Miyoushe | BetterGenshinImpact/Core/Config/OtherConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.ocrConfig | OCR配置 | Ocr | BetterGenshinImpact/Core/Config/OtherConfig.cs:37 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.restoreFocusOnLostEnabled | 调度器任务和部分独立任务，失去焦点，自动激活游戏窗口 | bool | BetterGenshinImpact/Core/Config/OtherConfig.cs:14 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.serverTimeZoneOffset | 服务器时区偏移量 | TimeSpan | BetterGenshinImpact/Core/Config/OtherConfig.cs:23 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.uiCultureInfoName | BGI界面语言名称 | string | BetterGenshinImpact/Core/Config/OtherConfig.cs:129 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| otherConfig.windowClassDetectPreferred | 窗口类名优先检测：原版仅靠进程名+MainWindowHandle 查找游戏窗口，该句柄为 0 或指向错误窗口时会找不到。开启后优先按窗口类名枚举检测，未命中再按进程枚举取客户区最大的可见窗口，仍不命中才回退原版方式。即时生效（每次查找窗口时读取），默认关闭，关闭时行为与旧版完全一致 | bool | BetterGenshinImpact/Core/Config/OtherConfig.cs:17 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.autoEatEnabled | 启用自动吃药功能 | bool | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:55 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.avatarConditions |  | ObservableCollection<Condition> | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.mapMatchingMethod | 地图追踪优先使用的特征匹配方式；影响所有地图追踪功能，重启后生效 | string | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.onlyInTeleportRecover | 只在传送传送点时复活 | bool | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:30 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.partyConditions | 地图追踪条件配置 | ObservableCollection<Condition> | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:23 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.recoverTiming | 低血量回复时机；选择低血量时的回复策略：任何路径点/只在传送点/不回复 | RecoverTiming | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:36 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| pathingConditionConfig.useGadgetIntervalMs | 使用小道具的间隔时间(ms) | int | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| quickTeleportConfig.enabled | 快速传送是否启用 | bool | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| quickTeleportConfig.hotkeyTpEnabled | 使用快捷键传送 | bool | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:31 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| quickTeleportConfig.teleportListClickDelay | 点击候选列表传送点的间隔时间(ms) | int | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:20 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| quickTeleportConfig.waitTeleportPanelDelay | 等待右侧传送弹出界面的时间(ms) 0.24 版本后，这个值可以设置为 0，因为识图时间变久了。0.24 版本前，建议设置为 100 | int | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:26 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| recordConfig.angle2DirectInputX | 视角每移动1度，需要DirectInput移动的单位 | double | BetterGenshinImpact/Core/Config/RecordConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| recordConfig.angle2MouseMoveByX | 视角每移动1度，需要MouseMoveBy的距离 用作脚本记录度数后转化的鼠标移动距离 | double | BetterGenshinImpact/Core/Config/RecordConfig.cs:13 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| recordConfig.isRecordCameraOrientation | 图像识别记录相机视角朝向 | bool | BetterGenshinImpact/Core/Config/RecordConfig.cs:25 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.autoUpdateBeforeCommandLineRun | 命令行启动时是否先自动更新已订阅脚本再执行命令 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:58 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.autoUpdateScriptRepoPeriod | 自动更新脚本仓库周期（天） | int | BetterGenshinImpact/Core/Config/ScriptConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.autoUpdateSubscribedScripts | 是否在启动时自动更新已订阅的脚本 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:55 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.customRepoUrl | 自定义渠道的URL | string | BetterGenshinImpact/Core/Config/ScriptConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.guideStatus | 仓库新手教程是否已阅读 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:52 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.lastUpdateScriptRepoTime | 上次更新脚本仓库时间 | DateTime | BetterGenshinImpact/Core/Config/ScriptConfig.cs:19 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.scriptRepoHintDotVisible | 脚本仓库按钮红点是否展示 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:23 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.selectedChannelName | 选择的更新渠道名称 | string | BetterGenshinImpact/Core/Config/ScriptConfig.cs:31 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.subscribedScriptPaths | 已订阅的脚本路径列表 | List<string> | BetterGenshinImpact/Core/Config/ScriptConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.webviewHeight | 仓库页面高度 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:40 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.webviewLeft | 仓库页面横向位置 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:43 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.webviewState | 仓库页面是否最大化 | WindowState | BetterGenshinImpact/Core/Config/ScriptConfig.cs:49 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.webviewTop | 仓库页面纵向位置 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| scriptConfig.webviewWidth | 仓库页面宽度 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:37 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| selectedOneDragonFlowConfigName | 一条龙选中使用的配置 | string | BetterGenshinImpact/Core/Config/AllConfig.cs:116 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| skillCdConfig.backgroundNormalColor | CD大于0.8s时计时器背景色（默认白色 #FFFFFFFF） | string | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:87 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.backgroundReadyColor | CD小于0.8s时计时器背景色（默认白色 #FFFFFFFF） | string | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:115 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.customCdList | 特定角色CD修正配置列表 | System.Collections.Generic.List<SkillCdRule> | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| skillCdConfig.enabled | 是否启用 | bool | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| skillCdConfig.gap | 计时器间隔 | double | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:63 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.hideWhenZero | 冷却为0时隐藏 | bool | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| skillCdConfig.px | 横坐标 | double | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:39 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.py | 纵坐标 | double | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:51 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.scale | 计时器缩放 | double | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:75 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.textNormalColor | CD大于0.8s时计时器文本色（默认 #DA4A23） | string | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:101 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.textReadyColor | CD小于0.8s时计时器文本色（默认 #5DCC17） | string | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:129 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| skillCdConfig.triggerOnSkillUse | 使用战技时触发（E键） | bool | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:27 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.hpRestoreDuration | 回血等待时间 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:112 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| tpConfig.mapDragUseRelativeMove | 大地图拖动使用相对鼠标移动 | bool | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:18 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.mapScaleFactor | 游戏坐标和 mapZoomLevel=1 时的像素比例因子。 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:150 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.mapZoomEnabled | 地图缩放开关 | bool | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:15 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.mapZoomInDistance | 地图放大的最大距离，单位：像素 | int | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| tpConfig.mapZoomOutDistance | 地图缩小的最小距离，单位：像素 | int | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:21 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| tpConfig.maxIterations | 移动最大次数 | int | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:145 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.maxZoomLevel | 最大缩放等级 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:84 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.country | 所在国家 | string? | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:35 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.id | 基本属性 | string | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:32 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.name | tp 名称 | string? | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:33 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.tranPosition | 实际传送的坐标 | decimal[] | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:44 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.tranX |  | double | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:45 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.tranY |  | double | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSeven.type | tp 类型 | string? | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:34 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSevenArea | 七天神像所在区域 | string | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:95 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSevenCountry | 七天神像所在国家 | string | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:98 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSevenPointX | 七天神像点位X坐标 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:89 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.reviveStatueOfTheSevenPointY | 七天神像点位Y坐标 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:92 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| tpConfig.teleportOperationDelayMilliseconds | 传送操作速度基准间隔，单位：ms | int | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:46 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。；有联动变更钩子 |
| tpConfig.tolerance | 允许的移动误差 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:140 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| triggerInterval | 触发器触发频率(ms) | int | BetterGenshinImpact/Core/Config/AllConfig.cs:61 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| wgcMinUpdateIntervalMs | WGC V2 帧率上限（毫秒，即最小更新间隔，限制 DWM 推帧频率以降低 GPU 占用） 0 = 不启用限流（默认）；仅 Windows 11 24H2 及以上系统生效 | int | BetterGenshinImpact/Core/Config/AllConfig.cs:68 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |
| wgcV2UseCpuConvert | WGC V2 使用 CPU 颜色转换（BGRA→BGR 由 CPU CvtColor 完成） 默认关闭 = GPU compute shader 打包 BGR24（回读量更小、CPU 零转换）；重启捕获后生效 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:75 | 源码已列出，运行桥尚未核验；当前没有可连接的桥快照，不将未核验记为缺失。 |

## 页面与窗口

| 页面／窗口源码 | 绑定命令 |
|---|---|
| BetterGenshinImpact/View/CaptureTestWindow.xaml |  |
| BetterGenshinImpact/View/Controls/CascadeSelector.xaml | ；代码后置事件：SelectionChanged=FirstLevelListView_SelectionChanged；SelectionChanged=SecondLevelListView_SelectionChanged |
| BetterGenshinImpact/View/Controls/CodeEditor/CodeBox.xaml |  |
| BetterGenshinImpact/View/Controls/Draggable/DraggableResizableItem.xaml |  |
| BetterGenshinImpact/View/Controls/Draggable/ResizeRotateChrome.xaml |  |
| BetterGenshinImpact/View/Controls/Draggable/SizeChrome.xaml |  |
| BetterGenshinImpact/View/Controls/Drawer/DrawerStyles.xaml |  |
| BetterGenshinImpact/View/Controls/Markdown/MarkdownView.xaml |  |
| BetterGenshinImpact/View/Controls/Markdown/Resources/MarkdownStyles.xaml |  |
| BetterGenshinImpact/View/Controls/MultiSelectComboBox.xaml | ；代码后置事件：PreviewKeyDown=Root_PreviewKeyDown；PreviewKeyDown=DropDownToggle_PreviewKeyDown；PreviewKeyDown=SearchTextBox_PreviewKeyDown；Click=SelectAllButton_Click；Click=ClearSelectionButton_Click；Click=ItemCheckBox_Click |
| BetterGenshinImpact/View/Controls/Overlay/AdjustableOverlayItemStyles.xaml |  |
| BetterGenshinImpact/View/Controls/Style/ListViewEx.xaml |  |
| BetterGenshinImpact/View/Controls/WpfUi/FaFontIconStyle.xaml |  |
| BetterGenshinImpact/View/HtmlMaskWindow.xaml |  |
| BetterGenshinImpact/View/MainWindow.xaml | {Binding ActivatedCommand}；{Binding LoadedCommand}；{Binding ClosingCommand}；{Binding OpenFeedCommand}；{Binding DismissRedeemCodeCommand}；{Binding OpenFeedCommand}；{Binding SwitchBackdropCommand}；{Binding HideCommand}；{Binding OpenChildSessionWindowCommand}；{Binding CheckUpdateCommand}；{Binding ExitCommand} |
| BetterGenshinImpact/View/MaskWindow.xaml | {Binding LoadedCommand}；{Binding WindowSizeChangedCommand}；{Binding OverlayLayoutCommittedCommand}；{Binding ExitOverlayLayoutEditModeCommand}；{Binding OverlayLayoutCommittedCommand}；{Binding ExitOverlayLayoutEditModeCommand}；{Binding OverlayLayoutCommittedCommand}；{Binding ExitOverlayLayoutEditModeCommand}；{Binding ToggleHiddenCommand}；{Binding ToggleHiddenCommand}；{Binding CloseCommand}；{Binding DataContext.OpenUrlCommand, RelativeSource={RelativeSource AncestorType=ItemsControl}}；{Binding ToggleMapPointPickerCommand}；{Binding HideAllMapPointsCommand}；{Binding ShowAllMapPointsCommand}；{Binding ToggleMapPointPickerCommand}；{Binding DataContext.SelectMapLabelItemCommand, RelativeSource={RelativeSource AncestorType=Window}}；{Binding ResetSelectedMapLabelSelectionCommand}；{Binding DataContext.SelectMapLabelItemCommand, RelativeSource={RelativeSource AncestorType=Window}} |
| BetterGenshinImpact/View/Pages/CommonSettingsPage.xaml | {Binding UiLanguageSelectionChangedCommand}；{Binding GameLangSelectionChangedCommand}；{Binding SelectMainBackgroundImageCommand}；{Binding ClearMainBackgroundImageCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding RefreshMaskSettingsCommand}；{Binding RefreshMaskSettingsCommand}；{Binding ResetMaskOverlayLayoutCommand}；{Binding ResetOverlayStyleCommand}；{Binding DataContext.GoToLogFolderCommand, RelativeSource={RelativeSource AncestorType=Page}}；{Binding DataContext.OpenCustomHtmlMaskEditorCommand, RelativeSource={RelativeSource AncestorType=Page}}；{Binding DataContext.OpenCustomHtmlMaskFolderCommand, RelativeSource={RelativeSource AncestorType=Page}}；{Binding SelectCrosshairImageCommand}；{Binding RefreshMaskSettingsCommand}；{Binding RefreshMaskSettingsCommand}；{Binding OpenRecognitionTemplateEditorFromImageCommand}；{Binding GoToFolderCommand}；{Binding SwitchTakenScreenshotEnabledCommand}；{Binding SwitchTakenScreenshotEnabledCommand}；{Binding GoToHotKeyPageCommand}；{Binding GoToRewardRecognitionFolderCommand}；{Binding OpenKeyBindingsWindowCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding QuestionButtonOnClickCommand}；{Binding SwitchMaskEnabledCommand}；{Binding SwitchMaskEnabledCommand}；{Binding PaddleOcrModelConfigChangedCommand}；{Binding CheckUpdateCommand}；{Binding CheckUpdateAlphaCommand}；{Binding OpenAboutWindowCommand} |
| BetterGenshinImpact/View/Pages/HomePage.xaml | {Binding LoadedCommand}；{Binding ChangeBannerImageCommand}；{Binding ChangeWebBannerImageCommand}；{Binding RefreshWebBannerImageCommand}；{Binding ResetBannerImageCommand}；{Binding GoToWikiUrlCommand}；{Binding CaptureModeDropDownChangedCommand}；{Binding OpenHardwareAccelerationSettingsCommand}；{Binding StartCaptureTestCommand}；{Binding ManualPickWindowCommand}；{Binding OpenDisplayAdvancedGraphicsSettingsCommand}；{Binding SelectInstallPathCommand}；{Binding OpenGameCommandLineDocumentCommand}；{Binding OpenChildSessionWindowCommand} |
| BetterGenshinImpact/View/Pages/HotkeyPage.xaml | {Binding SwitchHotKeyTypeCommand} |
| BetterGenshinImpact/View/Pages/JsListPage.xaml | {Binding GoToJsScriptUrlCommand}；{Binding OpenScriptsFolderCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding SetRightClickSelectionCommand}；{Binding SetRightClickSelectionCommand}；{Binding OpenScriptDetailDrawerCommand}；{Binding OpenScriptProjectFolderCommand}；{Binding StartRunCommand}；{Binding RefreshCommand}；{Binding DeleteScriptCommand}；{Binding DrawerVm.OnDrawerOpenedCommand}；{Binding DrawerVm.OnDrawerClosingCommand} |
| BetterGenshinImpact/View/Pages/KeyBindingsSettingsPage.xaml | {Binding FetchFromRegistryCommand} |
| BetterGenshinImpact/View/Pages/KeyMouseRecordPage.xaml | {Binding GoToKmScriptUrlCommand}；{Binding OpenScriptFolderCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding StartRecordCommand}；{Binding StopRecordCommand}；{Binding RelativeSource={RelativeSource FindAncestor, AncestorType={x:Type local:KeyMouseRecordPage}}, Path=DataContext.StartPlayCommand}；{Binding EditScriptCommand}；{Binding DeleteScriptCommand} |
| BetterGenshinImpact/View/Pages/MacroSettingsPage.xaml | {Binding GoToOneKeyMacroUrlCommand}；{Binding EditAvatarMacroCommand}；{Binding GoToHotKeyPageCommand}；{Binding GoToHotKeyPageCommand}；{Binding GoToHotKeyPageCommand}；{Binding GoToHotKeyPageCommand}；{Binding GoToHotKeyPageCommand}；{Binding GoToHotKeyPageCommand} |
| BetterGenshinImpact/View/Pages/MapPathingPage.xaml | {Binding GoToPathingUrlCommand}；{Binding OpenScriptsFolderCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding OpenSettingsCommand}；{Binding OpenDevToolsCommand}；{Binding SetRightClickSelectionCommand}；{Binding SetRightClickSelectionCommand}；{Binding OpenPathingDetailCommand}；{Binding StartCommand}；{Binding RefreshCommand}；{Binding DeleteCommand}；{Binding DrawerVm.OnDrawerOpenedCommand}；{Binding DrawerVm.OnDrawerClosingCommand} |
| BetterGenshinImpact/View/Pages/MusicPage.xaml | {Binding ViewModel.InitializeCommand}；{x:Static Slider.DecreaseLarge}；{x:Static Slider.IncreaseLarge}；{Binding ViewModel.OpenFolderCommand}；{Binding ViewModel.ChooseFolderCommand}；{Binding ViewModel.DeleteMusicFolderCommand}；{Binding ViewModel.OpenSettingsCommand}；{Binding ViewModel.SelectMusicFolderCommand}；{Binding ViewModel.RefreshCommand}；{Binding ViewModel.PlaySelectedCommand}；{Binding ViewModel.SeekCommand}；{Binding ViewModel.PreviousCommand}；{Binding ViewModel.PlayPauseCommand}；{Binding ViewModel.NextCommand}；{Binding ViewModel.StopCommand}；{Binding ViewModel.CyclePlaybackModeCommand} |
| BetterGenshinImpact/View/Pages/NotificationSettingsPage.xaml | {Binding OpenNotificationEventDocumentCommand}；{Binding SelectAllNotificationEventsCommand}；{Binding ClearNotificationEventSelectionCommand}；{Binding TestWebhookCommand}；{Binding TestWebSocketNotificationCommand}；{Binding TestWindowsUwpNotificationCommand}；{Binding TestFeishuNotificationCommand}；{Binding TestOneBotNotificationCommand}；{Binding TestWorkWeixinNotificationCommand}；{Binding TestEmailNotificationCommand}；{Binding TestBarkNotificationCommand}；{Binding TestTelegramNotificationCommand}；{Binding TestXxtuiNotificationCommand}；{Binding TestDingDingWebhookNotificationCommand}；{Binding TestDiscordWebhookNotificationCommand}；{Binding TestServerChanNotificationCommand}；{Binding TestMeowNotificationCommand}；{Binding TestGotifyNotificationCommand}；{Binding TestQqNotificationCommand}；{Binding TestWechatClawbotNotificationCommand} |
| BetterGenshinImpact/View/Pages/OneDragon/CraftPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/DailyCommissionPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/DailyRewardPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/DomainPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/ForgingPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/LeyLineBlossomPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/MailPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/SereniteaPotPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragon/TcgPage.xaml |  |
| BetterGenshinImpact/View/Pages/OneDragonFlowPage.xaml | {Binding LoadedCommand}；{Binding AddTaskGroupCommand}；{Binding DeleteTaskGroupCommand}；{Binding NextTaskGroupCommand}；{Binding ClearNextTaskGroupCommand}；{Binding OneKeyExecuteCommand}；{Binding ConfigDropDownChangedCommand}；{Binding AddConfigCommand}；{Binding RenameConfigCommand}；{Binding DeleteConfigCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding ResetAutoBossCompletedRunCountCommand}；代码后置事件：Click=AddTaskGroupButton_Click；Click=ConfirmButton_Click；Click=CancelButton_Click；Click=SereniteaPotTpType_Clicked；Click=SereniteaPotTpType_Clicked；Click=SereniteaPotTpType_Clicked |
| BetterGenshinImpact/View/Pages/ScriptControlPage.xaml | {Binding AddScriptGroupCommand}；{Binding DeleteScriptGroupCommand}；{Binding RenameScriptGroupCommand}；{Binding CopyScriptGroupCommand}；{Binding AddScriptGroupNextFlagCommand}；{Binding StartMultiScriptGroupCommand}；{Binding ContinueMultiScriptGroupCommand}；{Binding GoToScriptGroupUrlCommand}；{Binding AddScriptGroupCommand}；{Binding StartScriptGroupCommand}；{Binding AddJsScriptCommand}；{Binding AddPathingCommand}；{Binding AddKmScriptCommand}；{Binding AddShellCommand}；{Binding ClearTasksCommand}；{Binding OpenLogParseCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding UpdateTasksCommand}；{Binding ReverseTaskOrderCommand}；{Binding ExportMergerJsonsCommand}；{Binding OpenScriptGroupSettingsCommand}；{Binding AddJsScriptCommand}；{Binding AddPathingCommand}；{Binding AddKmScriptCommand}；{Binding AddShellCommand}；{Binding AddNextFlagCommand}；{Binding EditScriptCommonCommand}；{Binding EditJsScriptSettingsCommand}；{Binding OpenScriptFolderCommand}；{Binding DeleteScriptCommand}；{Binding DeleteScriptByFolderCommand} |
| BetterGenshinImpact/View/Pages/TaskSettingsPage.xaml | {Binding GoToAutoGeniusInvokationUrlCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding GoToAutoWoodUrlCommand}；{Binding GoToAutoFightUrlCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding OpenFightFolderCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding GoToAutoDomainUrlCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding OpenFightFolderCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding GoToAutoStygianOnslaughtUrlCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding OpenFightFolderCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding GoToAutoFishingUrlCommand}；{Binding GoToAutoLeyLineOutcropUrlCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding OpenFightFolderCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding GoToAutoMusicGameUrlCommand}；{Binding GoToAutoMusicGameUrlCommand}；{Binding GoToAutoMusicGameUrlCommand}；{Binding SwitchAutoComboRunCommand}；{Binding GoToArtifactSalvageUrlCommand}；{Binding OpenArtifactSalvageTestOCRWindowCommand}；{Binding GoToArtifactSalvageUrlCommand}；{Binding CopyArtifactSalvageJavaScriptFromRepositoryCommand}；{Binding GoToGetGridIconsFolderCommand}；{Binding GoToGetGridIconsUrlCommand}；{Binding GoToInventoryCountComparisonFolderCommand} |
| BetterGenshinImpact/View/Pages/TriggerSettingsPage.xaml | {Binding AutoPickModeChangedCommand}；{Binding AutoPickModeChangedCommand}；{Binding OpenBlacklistModeConfigCommand}；{Binding OpenWhitelistModeConfigCommand}；{Binding ToggleVoiceDiagnosticRecordingCommand}；{Binding GoToHotKeyPageCommand}；{Binding EditSkillCdConfigCommand} |
| BetterGenshinImpact/View/Pages/View/HardwareAccelerationView.xaml | {Binding OpenCacheFolderCommand} |
| BetterGenshinImpact/View/Pages/View/PathingConfigView.xaml | {Binding ClosingCommand}；{Binding DataContext.RemovePartyConditionConfigCommand, RelativeSource={RelativeSource AncestorType=ItemsControl}}；{Binding AddPartyConditionConfigCommand}；{Binding DataContext.RemoveAvatarConditionConfigCommand, RelativeSource={RelativeSource AncestorType=ItemsControl}}；{Binding AddAvatarConditionConfigCommand} |
| BetterGenshinImpact/View/Pages/View/ScriptGroupConfigView.xaml | {Binding GetExecutionOrderCommand}；{Binding AutoFightEnabledCheckedCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding OpenFightFolderCommand}；{Binding StrategyDropDownOpenedCommand}；{Binding GoToAutoEatUrlCommand} |
| BetterGenshinImpact/View/PickerWindow.xaml | ；代码后置事件：Loaded=Window_Loaded；PreviewKeyDown=FluentWindow_PreviewKeyDown；Click=cancelButton_Click |
| BetterGenshinImpact/View/Windows/AboutWindow.xaml | ；代码后置事件：Click=CloseButton_Click |
| BetterGenshinImpact/View/Windows/ArtifactOcrDialog.xaml | ；代码后置事件：Click=BtnOkClick；Click=BtnCancelClick |
| BetterGenshinImpact/View/Windows/AutoPickBlacklistConfigWindow.xaml | {Binding CancelCommand}；{Binding SaveCommand} |
| BetterGenshinImpact/View/Windows/AutoPickWhitelistConfigWindow.xaml | {Binding CancelCommand}；{Binding SaveCommand} |
| BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml | {Binding UpdateFromGitHostPlatformCommand}；{Binding UpdateFromSteambirdCommand}；{Binding EditCdkCommand}；{Binding UpdateFromMirrorChyanCommand}；{Binding BackgroundUpdateCommand}；{Binding OtherUpdateCommand}；{Binding IgnoreCommand}；{Binding CancelCommand} |
| BetterGenshinImpact/View/Windows/ChildSessionWindow.xaml | {Binding StartCommand}；{Binding HideCommand}；{Binding SwitchWindowCommand}；{Binding ToggleGameMouseModeCommand}；{Binding LaunchBetterGiCommand}；{Binding ToggleSmallWindowModeCommand}；{Binding SelectDefaultResolutionCommand}；{Binding UseAdaptiveCommand}；{Binding UseOneToOneCommand}；{Binding ToggleKeepAspectRatioCommand}；{Binding ToggleSendSystemShortcutsToRemoteCommand}；{Binding ShowDesktopCommand}；{Binding ShowTaskViewCommand}；{Binding LaunchExecutableCommand}；{Binding ToggleAudioMutedCommand}；{Binding ToggleTopmostCommand}；{Binding ToggleTopmostCommand}；{Binding StartCommand}；{Binding OpenDesktopHelpCommand} |
| BetterGenshinImpact/View/Windows/CustomHtmlMaskEditorWindow.xaml | {Binding SaveCustomHtmlMaskCommand}；{Binding ToggleCustomHtmlMaskPreviewCommand}；{Binding RestoreDefaultCustomHtmlMaskCommand}；{Binding OpenCustomHtmlMaskFolderCommand}；{Binding CloseCommand} |
| BetterGenshinImpact/View/Windows/Editable/ScriptGroupProjectEditor.xaml |  |
| BetterGenshinImpact/View/Windows/FeedWindow.xaml | {Binding DataContext.CopyItemCodesCommand, RelativeSource={RelativeSource AncestorType=ItemsControl}}；{Binding DataContext.AutoRedeemItemCommand, RelativeSource={RelativeSource AncestorType=ItemsControl}}；{Binding SelectCnServerCommand}；{Binding SelectGlobalServerCommand}；{Binding GetLiveRedeemCodesCommand}；代码后置事件：Click=BtnCloseClick |
| BetterGenshinImpact/View/Windows/ImageEditWindow.xaml | ；代码后置事件：Click=RotateLeft_Click；Click=RotateRight_Click；Click=ResetCrop_Click；Click=CancelButton_Click；Click=UseOriginalButton_Click；Click=SaveButton_Click |
| BetterGenshinImpact/View/Windows/JsonMonoDialog.xaml | {Binding SaveCommand}；{Binding CloseCommand} |
| BetterGenshinImpact/View/Windows/KeyBindingsWindow.xaml |  |
| BetterGenshinImpact/View/Windows/MapLabelSearchWindow.xaml | ；代码后置事件：Click=CloseButton_Click |
| BetterGenshinImpact/View/Windows/MapPathingDevWindow.xaml | {Binding OpenMapViewerCommand}；{Binding OpenMapEditorCommand} |
| BetterGenshinImpact/View/Windows/MapViewer.xaml |  |
| BetterGenshinImpact/View/Windows/MusicSettingsWindow.xaml | {Binding DataContext.UpdateTrackSelectionCommand, RelativeSource={RelativeSource AncestorType=ui:FluentWindow}}；{Binding SaveProfilesCommand} |
| BetterGenshinImpact/View/Windows/PictureInPictureWindow.xaml |  |
| BetterGenshinImpact/View/Windows/PromptDialog.xaml | ；代码后置事件：Click=BtnOkClick；Click=BtnCancelClick |
| BetterGenshinImpact/View/Windows/RecognitionTemplateEditorWindow.xaml | {Binding FitImageCommand}；{Binding BrowseRecognitionJsonCommand}；{Binding BrowseAssetsRootCommand}；{Binding NormalizeTemplateFileNameCommand}；{Binding CancelCommand}；{Binding SaveCommand} |
| BetterGenshinImpact/View/Windows/RepoUpdateDialog.xaml | ；代码后置事件：Click=PrimaryButton_Click；Click=SecondaryButton_Click |
| BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml | {Binding UpdateRepoCommand}；{Binding ResetRepoCommand}；{Binding ImportLocalScriptsRepoZipCommand}；{Binding OpenLocalScriptRepoCommand}；{Binding UpdateSubscribedScriptsCommand} |
| BetterGenshinImpact/View/Windows/SkillCdConfigWindow.xaml | ；代码后置事件：Click=OnDeleteClick；Click=OnAddClick |
| BetterGenshinImpact/View/Windows/ThemedMessageBox.xaml | ；代码后置事件：Click=PrimaryButton_Click；Click=SecondaryButton_Click；Click=CloseButton_Click |
| BetterGenshinImpact/View/Windows/WebImageInput.xaml | {Binding SubmitWebImageUrlCommand} |
| BetterGenshinImpact/View/Windows/WelcomeDialog.xaml | ；代码后置事件：Click=BtnOkClick |

## 脚本资源模型字段

| 模型字段 | 类型 | 说明 | 源码位置 |
|---|---|---|---|
| BetterGenshinImpact.Core.Script.Group.ScriptGroup.Config | ScriptGroupConfig |  | BetterGenshinImpact/Core/Script/Group/ScriptGroup.cs:24 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroup.Index | int |  | BetterGenshinImpact/Core/Script/Group/ScriptGroup.cs:19 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroup.Name | string |  | BetterGenshinImpact/Core/Script/Group/ScriptGroup.cs:21 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroup.Projects | ObservableCollection<ScriptGroupProject> |  | BetterGenshinImpact/Core/Script/Group/ScriptGroup.cs:27 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupConfig.EnableShellConfig | bool | 是否启用 Shell 执行配置 | BetterGenshinImpact/Core/Script/Group/ScriptGroupConfig.cs:22 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupConfig.PathingConfig | PathingPartyConfig |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupConfig.cs:10 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupConfig.ShellConfig | ShellConfig | Shell 执行配置 | BetterGenshinImpact/Core/Script/Group/ScriptGroupConfig.cs:16 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.AllowJsHTTPHash | string? |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:109 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.AllowJsNotification | bool? |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:106 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.FolderName | string | 理论上是当前类型脚本根目录到脚本文件所在目录的相对路径 但是： 1. JS 脚本的文件名是内部的名称，文件夹名脚本所在文件夹，这个也是唯一标识 2. KeyMouse 脚本的文件名和文件夹名相同，文件夹名暂时无意义 3. Pathing 文件名就是实际脚本的文件名，文件夹名是脚本所在的相对目录 | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:44 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.Index | int |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:29 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.JsScriptSettingsObject | ExpandoObject? |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:74 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.Name | string | 理论上是文件名 | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:35 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.RunNum | int |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:68 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.Schedule | string | 执行周期 不在 ScheduleDescriptions 中则会被视为自定义Cron表达式 | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:62 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.Status | string |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:52 |
| BetterGenshinImpact.Core.Script.Group.ScriptGroupProject.Type | string |  | BetterGenshinImpact/Core/Script/Group/ScriptGroupProject.cs:46 |
| BetterGenshinImpact.Core.Script.Project.Author.Link | string |  | BetterGenshinImpact/Core/Script/Project/Author.cs:6 |
| BetterGenshinImpact.Core.Script.Project.Author.Name | string |  | BetterGenshinImpact/Core/Script/Project/Author.cs:5 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Authors | List<Author> |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:23 |
| BetterGenshinImpact.Core.Script.Project.Manifest.BgiVersion | string? |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:21 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Description | string |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:22 |
| BetterGenshinImpact.Core.Script.Project.Manifest.HttpAllowedUrls | string[] |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:29 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Library | string[] |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:27 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Main | string |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:24 |
| BetterGenshinImpact.Core.Script.Project.Manifest.ManifestVersion | int |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:18 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Name | string |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:19 |
| BetterGenshinImpact.Core.Script.Project.Manifest.SavedFiles | string[] |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:28 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Scripts | string[] |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:26 |
| BetterGenshinImpact.Core.Script.Project.Manifest.SettingsUi | string |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:25 |
| BetterGenshinImpact.Core.Script.Project.Manifest.Version | string |  | BetterGenshinImpact/Core/Script/Project/Manifest.cs:20 |
| BetterGenshinImpact.Core.Script.Project.ScriptProject.FolderName | string |  | BetterGenshinImpact/Core/Script/Project/ScriptProject.cs:27 |
| BetterGenshinImpact.Core.Script.Project.ScriptProject.Manifest | Manifest |  | BetterGenshinImpact/Core/Script/Project/ScriptProject.cs:25 |
| BetterGenshinImpact.Core.Script.Project.ScriptProject.ManifestFile | string |  | BetterGenshinImpact/Core/Script/Project/ScriptProject.cs:23 |
| BetterGenshinImpact.Core.Script.Project.ScriptProject.ProjectPath | string |  | BetterGenshinImpact/Core/Script/Project/ScriptProject.cs:22 |

## JavaScript 宿主 API

这些是脚本运行环境中的 API，不等同于 Agent 已有直接桥入口；用 bgi-javascript 追踪脚本调用。

| 方法 | 参数 | 返回 | 说明／源码 |
|---|---|---|---|
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.IsExists | string subPath | bool | 判断 AutoPathing 目录下的路径是否存在 / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:73 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.IsFile | string subPath | bool | 判断 AutoPathing 目录下的路径是否为文件 / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:80 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.IsFolder | string subPath | bool | 判断 AutoPathing 目录下的路径是否为文件夹 / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:87 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.ReadPathSync | string subPath | string[] | 读取 AutoPathing 目录下指定文件夹的内容（非递归方式） 目录不存在时返回空数组，不会自动创建目录 / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:95 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.ReadTextSync | string subPath | string | 读取 AutoPathing 目录下指定文件的文本内容 / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:102 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.Run | string json | Task |  / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:24 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.RunFile | string path | Task |  / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:44 |
| BetterGenshinImpact.Core.Script.Dependence.AutoPathingScript.RunFileFromUser | string path | Task | 从已订阅的内容中获取文件 / BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs:62 |
| BetterGenshinImpact.Core.Script.Dependence.CustomHostFunctions.NewVarOfArr | int dimensions | object | 创建指定维度的交错数组变量 / BetterGenshinImpact/Core/Script/Dependence/CustomHostFunctions.cs:15 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.AddTimer | RealtimeTimer timer | void | 添加实时任务,会清理之前的所有任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:52 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.AddTrigger | RealtimeTimer timer | void | 添加实时任务,不会清理之前的任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:82 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.ClearAllTriggers |  | void | 清理所有实时任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:71 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.GetLinkedCancellationToken |  | CancellationToken |  / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:314 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.GetLinkedCancellationTokenSource |  | CancellationTokenSource |  / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:307 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunAutoBossTask | AutoBossParam param, CancellationToken? customCt | Task<Dictionary<string, int>> | 运行自动首领讨伐任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:342 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunAutoDomainTask | AutoDomainParam param, CancellationToken? customCt | Task<Dictionary<string, int>> | 运行自动秘境任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:325 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunAutoFightTask | AutoFightParam param, CancellationToken? customCt | Task | 运行自动战斗任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:359 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunAutoLeyLineOutcropTask | AutoLeyLineOutcropParam param, CancellationToken? customCt | Task | 运行自动地脉花任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:403 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunAutoStygianOnslaughtTask | AutoStygianOnslaughtParam param, CancellationToken? customCt | Task | 运行自动幽境危战任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:421 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunCombatScript | string script, string? avatarName, CancellationToken? customCt | Task | 运行简易战斗策略脚本。 使用策略语言直接控制角色执行动作（如 e、q、attack 等），适合快速操作。 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:379 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunCountInventoryItemTask | CountInventoryItemParam param, CancellationToken? customCt | Task<object?> | 运行背包物品计数任务。 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:438 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunTask |  | void |  / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:43 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunTask | SoloTask soloTask, CancellationToken? customCt | Task<object?> | 运行独立任务 / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:124 |
| BetterGenshinImpact.Core.Script.Dependence.Dispatcher.RunTask | SoloTask soloTask, CancellationTokenSource customCts | Task |  / BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs:101 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.AutoFishing | int fishingTimePolicy | Task | 钓鱼 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:457 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.BlessingOfTheWelkinMoon |  | Task | 自动点击空月祝福 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:367 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.ChooseTalkOption | string option, int skipTimes, bool isOrange | Task | 持续对话并选择目标选项 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:379 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.ClaimBattlePassRewards |  | Task | 一键领取纪行奖励 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:388 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.ClaimEncounterPointsRewards |  | Task | 领取长效历练点奖励 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:397 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.ClearPartyCache |  | void | 清除当前调度器的队伍缓存 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:357 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.ClickMapPoint | double x, double y, string? forceCountry | Task | 点击大地图上的指定坐标。 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:135 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.CraftMaterial | string materialName, int quantity, string? materialType | Task<CraftMaterialResult> | 在当前已打开的合成界面中合成指定材料。 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:439 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetBigMapZoomLevel |  | double | 获取当前大地图缩放等级 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:175 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetCameraOrientation |  | float |  / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:244 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromBigMap |  | Point2f? | 获取当前在大地图上的位置坐标 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:213 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromBigMap | string mapName | Point2f? | 获取当前在大地图上的位置坐标 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:224 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromMap |  | Point2f? | 获取当前在小地图上的位置坐标 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:234 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromMap | string mapName, float x, float y | Point2f? | 获取当前在小地图上的位置坐标, 局部匹配, 需要世界坐标, 在坐标附近匹配, 失败不进行全局匹配 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:281 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromMap | string mapName, int cacheTimeMs | Point2f? | 获取当前在小地图上的位置坐标，如果缓存时间内有匹配成功的坐标优先返回缓存坐标，否则调用NavigationInstance的getPositionStable / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:256 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromMapWithMatchingMethod | string matchingMethod | Point2f? |  / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:239 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GetPositionFromMapWithMatchingMethod | string mapName, string matchingMethod, int cacheTimeMs | Point2f? |  / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:262 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GoCraftResin | string country | Task | 前往合成台合成浓缩树脂 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:427 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GoToAdventurersGuild | string country | Task | 前往冒险家协会领取奖励 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:407 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.GoToCraftingBench | string country | Task | 前往合成台 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:417 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.MoveIndependentMapTo | int x, int y, string mapName, string? forceCountry | Task | 移动大地图到指定坐标 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:153 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.MoveMapTo | double x, double y, string? forceCountry | Task | 移动大地图到指定坐标 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:118 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Relogin |  | Task | 重新登录原神 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:474 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.ReturnMainUi |  | Task | 返回主界面 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:448 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.SetBigMapZoomLevel | double zoomLevel | Task | 将大地图缩放等级设置为指定值 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:193 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.SetTime | int hour, int minute, bool skip | Task | 调整时间 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:495 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.SetTime | string hour, string minute, bool skip | Task | 调整时间 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:511 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.SwitchCharacter | string slot1, string slot2, string slot3, string slot4, bool usePhysicalSlots | Task<bool> | 按槽位重组当前队伍角色。 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:330 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.SwitchParty | string partyName | Task<bool> | 切换队伍 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:303 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Tp | double x, double y | Task | 传送到指定位置 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:70 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Tp | double x, double y, bool force | Task |  / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:80 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Tp | double x, double y, string mapName, bool force | Task |  / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:75 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Tp | string x, string y | Task | 传送到指定位置 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:91 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Tp | string x, string y, bool force | Task |  / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:98 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.TpToStatueOfTheSeven |  | Task | 传送到用户指定的七天神像 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:203 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.Uid |  | Task<int> | 通过 OCR 识别当前角色的 UID / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:52 |
| BetterGenshinImpact.Core.Script.Dependence.Genshin.WonderlandCycle |  | Task | 进出千星奇域 / BetterGenshinImpact/Core/Script/Dependence/Genshin.cs:483 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.CaptureGameRegion |  | ImageRegion |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:257 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.Click | int x, int y | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:197 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.GetAvatars |  | string[] |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:262 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.GetGameMetrics |  | double[] |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:170 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.GetVersion |  | string |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:28 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.InputText | string text | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:274 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.KeyDown | string key | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:35 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.KeyPress | string key | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:103 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.KeyUp | string key | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:69 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.LeftButtonClick |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:203 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.LeftButtonDown |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:208 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.LeftButtonUp |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:213 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.MiddleButtonClick |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:233 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.MiddleButtonDown |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:238 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.MiddleButtonUp |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:243 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.MoveMouseBy | int x, int y | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:175 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.MoveMouseTo | int x, int y | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:183 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.RightButtonClick |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:218 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.RightButtonDown |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:223 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.RightButtonUp |  | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:228 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.SetGameMetrics | int width, int height, double dpi | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:157 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.Sleep | int millisecondsTimeout | Task |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:23 |
| BetterGenshinImpact.Core.Script.Dependence.GlobalMethod.VerticalScroll | int scrollAmountInClicks | void |  / BetterGenshinImpact/Core/Script/Dependence/GlobalMethod.cs:248 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Close | string id | bool | 关闭指定窗口 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:113 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.CloseAll |  | void | 关闭所有由本实例打开的窗口 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:123 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Dispose |  | void |  / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:404 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Exists | string id | bool | 窗口是否存在 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:146 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.GetClickThrough | string windowId | bool | 获取窗口的点击穿透状态 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:163 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.GetWindowIds |  | string[] | 获取所有窗口ID / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:141 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Poll | string windowId | string? | 轮询来自HTML的消息（非阻塞） / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:303 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.PollAll | string windowId | string | 批量获取来自HTML的所有消息 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:316 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Receive | string windowId, int timeoutMs | Task<string?> | 等待接收来自HTML的一条消息 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:279 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Request | string windowId, string url, string jsonData, int timeoutMs | Task<string?> | 发送请求到HTML并等待响应 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:229 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Respond | string windowId, string requestId, string jsonData | void | 响应 HTML 页面通过 window.htmlMask.request(...) 发起的请求。 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:204 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Send | string windowId, string url, string jsonData | void | 发送消息到HTML（单向推送） / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:184 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.SetClickThrough | string windowId, bool enabled | void | 设置窗口的点击穿透模式 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:153 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.Show | string url, string? id | string | 显示HTML遮罩窗口 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:75 |
| BetterGenshinImpact.Core.Script.Dependence.HtmlMask.ToggleClickThrough | string windowId | void | 切换窗口的点击穿透模式 / BetterGenshinImpact/Core/Script/Dependence/HtmlMask.cs:172 |
| BetterGenshinImpact.Core.Script.Dependence.Http.Request | string method, string url, string? body, string? headersJson | Task<HttpReponse> | 执行HTTP请求 / BetterGenshinImpact/Core/Script/Dependence/Http.cs:62 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.Dispose |  | void |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:451 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.OnKeyDown | ScriptObject callback, bool useCodeOnly | void | 注册键盘按下事件回调 / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:388 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.OnKeyUp | ScriptObject callback, bool useCodeOnly | void | 注册键盘释放事件回调 / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:401 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.OnMouseDown | ScriptObject callback | void |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:409 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.OnMouseMove | ScriptObject callback, int interval | void | 注册鼠标移动事件回调 / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:424 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.OnMouseUp | ScriptObject callback | void |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:414 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.OnMouseWheel | ScriptObject callback | void |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:431 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseHook.RemoveAllListeners |  | void |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseHook.cs:436 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseScript.Run | string json | Task |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseScript.cs:8 |
| BetterGenshinImpact.Core.Script.Dependence.KeyMouseScript.RunFile | string path | Task |  / BetterGenshinImpact/Core/Script/Dependence/KeyMouseScript.cs:13 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.CreateDirectory | string folderPath | bool | 创建指定路径的目录，如果已存在则跳过 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:59 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.IsExists | string path | bool | 判断指定的文件或目录是否存在 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:123 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.IsFile | string path | bool | 判断指定路径是否为文件 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:104 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.IsFolder | string path | bool | 判断指定路径是否为文件夹。 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:81 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.ReadImageMatSync | string path | Mat | 读取Mat图片 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:213 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.ReadImageMatWithResizeSync | string path, double width, double height, int interpolation | Mat | 读取图像文件为Mat对象，并调整到指定尺寸 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:249 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.ReadPathSync | string folderPath | string[] | 读取指定文件夹内所有文件和文件夹的路径（非递归方式）。 目录不存在时返回空数组 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:22 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.ReadText | string path | Task<string> | Read all text from a file. / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:170 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.ReadText | string path, dynamic callbackFunc | Task<string> | Read all text from a file. / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:192 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.ReadTextSync | string path | string | Read all text from a file. / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:150 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.RenamePathSync | string oldPath, string newPath | bool | 重命名文件或文件夹（相对于根目录） / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:519 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.WriteImageSync | string path, Mat mat | bool | 同步写入图片到文件（默认PNG格式） / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:443 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.WriteText | string path, string content, bool append | Task<bool> | 异步写入文本到文件 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:374 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.WriteText | string path, string content, dynamic callbackFunc, bool append | Task<bool> | 异步写入文本到文件（带回调） / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:408 |
| BetterGenshinImpact.Core.Script.Dependence.LimitedFile.WriteTextSync | string path, string content, bool append | bool | 同步写入文本到文件 / BetterGenshinImpact/Core/Script/Dependence/LimitedFile.cs:341 |
| BetterGenshinImpact.Core.Script.Dependence.Log.Debug | string? message, object?[] args | void |  / BetterGenshinImpact/Core/Script/Dependence/Log.cs:9 |
| BetterGenshinImpact.Core.Script.Dependence.Log.Error | string? message, object?[] args | void |  / BetterGenshinImpact/Core/Script/Dependence/Log.cs:24 |
| BetterGenshinImpact.Core.Script.Dependence.Log.Info | string? message, object?[] args | void |  / BetterGenshinImpact/Core/Script/Dependence/Log.cs:14 |
| BetterGenshinImpact.Core.Script.Dependence.Log.Warn | string? message, object?[] args | void |  / BetterGenshinImpact/Core/Script/Dependence/Log.cs:19 |
| BetterGenshinImpact.Core.Script.Dependence.Notification.Error | string message | void | 发送错误通知 / BetterGenshinImpact/Core/Script/Dependence/Notification.cs:93 |
| BetterGenshinImpact.Core.Script.Dependence.Notification.Send | string message | void | 发送成功通知 / BetterGenshinImpact/Core/Script/Dependence/Notification.cs:68 |
| BetterGenshinImpact.Core.Script.Dependence.ServerTime.GetServerTimeZoneOffset |  | int | 获取服务器时区偏移量 / BetterGenshinImpact/Core/Script/Dependence/ServerTime.cs:15 |
| BetterGenshinImpact.Core.Script.Dependence.Simulator.PostMessage.Click |  | void |  / BetterGenshinImpact/Core/Script/Dependence/Simulator/PostMessage.cs:28 |
| BetterGenshinImpact.Core.Script.Dependence.Simulator.PostMessage.KeyDown | string key | void |  / BetterGenshinImpact/Core/Script/Dependence/Simulator/PostMessage.cs:13 |
| BetterGenshinImpact.Core.Script.Dependence.Simulator.PostMessage.KeyPress | string key | void |  / BetterGenshinImpact/Core/Script/Dependence/Simulator/PostMessage.cs:23 |
| BetterGenshinImpact.Core.Script.Dependence.Simulator.PostMessage.KeyUp | string key | void |  / BetterGenshinImpact/Core/Script/Dependence/Simulator/PostMessage.cs:18 |
| BetterGenshinImpact.Core.Script.Dependence.StrategyFile.IsExists | string subPath | bool | 判断 User\AutoFight 目录下的路径是否存在 / BetterGenshinImpact/Core/Script/Dependence/StrategyFile.cs:34 |
| BetterGenshinImpact.Core.Script.Dependence.StrategyFile.IsFile | string subPath | bool | 判断 User\AutoFight 目录下的路径是否为文件 / BetterGenshinImpact/Core/Script/Dependence/StrategyFile.cs:27 |
| BetterGenshinImpact.Core.Script.Dependence.StrategyFile.IsFolder | string subPath | bool | 判断 User\AutoFight 目录下的路径是否为文件夹 / BetterGenshinImpact/Core/Script/Dependence/StrategyFile.cs:20 |
| BetterGenshinImpact.Core.Script.Dependence.StrategyFile.ReadPathSync | string subPath | string[] | 读取 User\AutoFight 目录下指定文件夹的内容（非递归方式） 目录不存在时返回空数组，不会自动创建目录 / BetterGenshinImpact/Core/Script/Dependence/StrategyFile.cs:42 |

## 稳定 BGI 操作入口

| 入口 | 桥实现 | 当前运行桥是否登记 |
|---|---|---|
| bgi.commit_settings | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.get_setting | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.get_setting_change | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.invoke_command | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.list_commands | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.list_setting_changes | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.list_setting_sections | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.preview_settings | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.rollback_settings | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.search_settings | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.set_setting | bgi-bridge/managed/Tools/CatalogTools.cs | 否／需核对更新版本 |
| bgi.create_command_argument | bgi-bridge/managed/Tools/CommandTargetTools.cs | 否／需核对更新版本 |
| bgi.create_command_target | bgi-bridge/managed/Tools/CommandTargetTools.cs | 否／需核对更新版本 |
| bgi.list_command_targets | bgi-bridge/managed/Tools/CommandTargetTools.cs | 否／需核对更新版本 |
| bgi.release_command_target | bgi-bridge/managed/Tools/CommandTargetTools.cs | 否／需核对更新版本 |
| bgi.set_game_resolution | bgi-bridge/managed/Tools/GameResolutionTools.cs | 否／需核对更新版本 |
| bgi.get_script_errors | bgi-bridge/managed/Tools/HostLogTools.cs | 否／需核对更新版本 |
| bgi.read_host_log | bgi-bridge/managed/Tools/HostLogTools.cs | 否／需核对更新版本 |
| bgi.prepare_js_group | bgi-bridge/managed/Tools/JavaScriptPreparationTools.cs | 否／需核对更新版本 |
| bgi.list_pages | bgi-bridge/managed/Tools/NavigationTools.cs | 否／需核对更新版本 |
| bgi.open_page | bgi-bridge/managed/Tools/NavigationTools.cs | 否／需核对更新版本 |
| bgi.exit_game | bgi-bridge/managed/Tools/OneDragonTools.cs | 否／需核对更新版本 |
| bgi.run_one_dragon | bgi-bridge/managed/Tools/OneDragonTools.cs | 否／需核对更新版本 |
| bgi.prepare_pathing_group | bgi-bridge/managed/Tools/PathingPreparationTools.cs | 否／需核对更新版本 |
| bgi.delete_script_group | bgi-bridge/managed/Tools/ScriptGroupDeletionTools.cs | 否／需核对更新版本 |
| bgi.run_script_group | bgi-bridge/managed/Tools/ScriptGroupTools.cs | 否／需核对更新版本 |
| bgi.read_script_repository_file | bgi-bridge/managed/Tools/ScriptRepositoryTools.cs | 否／需核对更新版本 |
| bgi.search_script_repository | bgi-bridge/managed/Tools/ScriptRepositoryTools.cs | 否／需核对更新版本 |
| bgi.subscribe_script_resources | bgi-bridge/managed/Tools/ScriptRepositoryTools.cs | 否／需核对更新版本 |
| bgi.update_subscribed_scripts | bgi-bridge/managed/Tools/ScriptRepositoryTools.cs | 否／需核对更新版本 |
| bgi.get_status | bgi-bridge/managed/Tools/StatusTools.cs | 否／需核对更新版本 |
| bgi.ping | bgi-bridge/managed/Tools/StatusTools.cs | 否／需核对更新版本 |
| bgi.probe | bgi-bridge/managed/Tools/StatusTools.cs | 否／需核对更新版本 |
| bgi.start_game | bgi-bridge/managed/Tools/StatusTools.cs | 否／需核对更新版本 |
| bgi.wait_ready | bgi-bridge/managed/Tools/StatusTools.cs | 否／需核对更新版本 |
| bgi.stop_current_task | bgi-bridge/managed/Tools/TaskStopTools.cs | 否／需核对更新版本 |
