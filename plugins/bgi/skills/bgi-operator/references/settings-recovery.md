# 设置恢复

普通“撤销刚才修改”可用 rollback_settings。它只在目标项仍等于该次修改后的值时回退，不覆盖后改内容。

用户明确要把特定设置恢复到历史某次记录的修改前值时：list_setting_changes / get_setting_change 定位 changeId、recordVersion 和 differences；按用户指定的 paths 读取 preview_setting_restore。预览显示当前值、恢复值、后来是否改过及联动项，敏感内容遮蔽。canApply=true 才能确认并 commit_settings(planId)。目标项或源记录在预览后变化时拒绝；无关设置的后续修改保留。

恢复本身生成 setting-restore 记录，ParentChangeId 指向来源，修改前值可用于撤销本次恢复。恢复记录不是脚本、路线或配置组文件备份。

完整配置备份替换 config.json 的全部内容，只在对应 BetterGI 完全退出时可做；用于配置损坏或明确的全量恢复。逐项恢复不要求关闭应用，但独立任务运行时不得修改设置。当前连接没有新版恢复接口时说明需要重连／重启，不能直接写 User/config.json 绕开内存保存。
