package dev.sleepydoll.preview

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.sleepydoll.ui.*
import org.json.JSONObject
import java.time.LocalDate
import java.time.LocalTime

object NativeComponents {
    val ids = setOf("button", "icon-button", "text-field", "textarea", "search-field", "switch", "checkbox", "slider", "select", "tabs", "badge", "avatar", "progress", "setting-row", "section", "empty-state", "task-card", "message-bubble", "composer", "dialog", "notice", "list-item",
        "icon", "radio-group", "filter-chips", "stepper", "range-slider", "date-picker", "time-picker", "top-app-bar", "bottom-navigation", "floating-action-button", "step-indicator", "bottom-sheet", "action-sheet", "dropdown-menu", "snackbar", "circular-progress", "skeleton", "divider", "accordion", "stat-card", "profile-card", "file-item", "upload-card").map { "sleepy-doll-android:$it" }.toSet()

    @Composable
    fun Render(model: PreviewModel) {
        val p = model.props
        val patch: (String, Any?) -> Unit = { key, value -> PreviewSession.patch(model.session, key, value) }
        val full = Modifier.fillMaxWidth()
        val enabled = !p.optBoolean("disabled")
        when (model.id.substringAfter(':')) {
            "button" -> SleepyButton(p.text("label", "继续"), { patch("clicks", p.optInt("clicks") + 1) }, enabled = enabled, loading = p.optBoolean("loading"), variant = when (p.optString("variant")) { "secondary" -> ButtonVariant.Secondary; "text" -> ButtonVariant.Text; else -> ButtonVariant.Primary })
            "icon-button" -> SleepyIconButton(p.text("label", "添加"), { patch("clicks", p.optInt("clicks") + 1) }, enabled = enabled) { Text("+", style = MaterialTheme.typography.titleLarge) }
            "text-field", "textarea" -> SleepyTextField(p.text("value"), { patch("value", it) }, p.text("label", "名称"), full, placeholder = p.text("placeholder"), enabled = enabled, error = p.optString("error").ifBlank { null }, multiline = model.id.endsWith(":textarea"), password = p.optBoolean("password"))
            "search-field" -> SleepySearchField(p.text("value"), { patch("value", it) }, full, p.text("placeholder", "搜索"), enabled)
            "switch" -> SleepySwitch(p.optBoolean("checked"), { patch("checked", it) }, p.text("label", "启用"), enabled = enabled)
            "checkbox" -> SleepyCheckbox(p.optBoolean("checked"), { patch("checked", it) }, p.text("label", "保存配置"), enabled = enabled)
            "slider" -> SleepySlider(p.optDouble("value", 40.0).toFloat().coerceIn(0f, 100f), { patch("value", it) }, full, enabled)
            "select" -> SleepySelect(p.text("value", "default"), { patch("value", it) }, p.text("label", "选择配置"), options(p), full, enabled)
            "tabs" -> SleepyTabs(p.text("value", "default"), { patch("value", it) }, options(p), full)
            "badge" -> SleepyBadge(p.text("label", "已保存"), active = p.optBoolean("active"))
            "avatar" -> SleepyAvatar(p.text("name", "木偶"))
            "progress" -> SleepyProgress(p.optDouble("progress", .4).toFloat(), full, p.optBoolean("indeterminate"))
            "setting-row" -> SleepySettingRow(p.text("title", "启用扩展"), description = p.text("description")) { SleepySwitch(p.optBoolean("checked"), { patch("checked", it) }, p.text("title", "启用扩展"), enabled = enabled) }
            "section" -> SleepySection(p.text("title", "外观"), description = p.text("description")) { SleepySettingRow("深色主题") { SleepySwitch(p.optBoolean("checked"), { patch("checked", it) }, "深色主题") } }
            "empty-state" -> SleepyEmptyState(p.text("title", "还没有快捷任务"), p.text("description", "创建配置后将在这里显示。")) { SleepyButton(p.text("action", "创建任务"), { patch("clicks", p.optInt("clicks") + 1) }) }
            "task-card" -> SleepyTaskCard(p.text("title", "采集路线"), p.text("description", "运行已保存的配置"), if (p.optBoolean("running")) "运行中" else "已就绪", { patch("running", !p.optBoolean("running")) }, running = p.optBoolean("running"), enabled = enabled, pinned = p.optBoolean("pinned"), onOpen = { patch("opened", true) })
            "message-bubble" -> SleepyMessageBubble(p.text("text", "开始一项新任务。"), user = p.optBoolean("user"))
            "composer" -> SleepyComposer(p.text("value"), { patch("value", it) }, { patch("submitted", true) }, enabled = enabled)
            "dialog" -> {
                SleepyButton("打开弹窗", { patch("open", true) })
                if (p.optBoolean("open", true)) SleepyDialog(p.text("title", "确认操作"), p.text("text", "确定保存这份配置吗？"), { patch("open", false) }, { patch("confirmed", true); patch("open", false) }, p.text("confirmLabel", "保存"), busy = p.optBoolean("busy"))
            }
            "notice" -> SleepyNotice(p.text("text", "配置已保存"), actionLabel = p.text("action", "查看"), onAction = { patch("clicked", true) })
            "list-item" -> SleepyListItem(p.text("title", "本地配置"), p.text("description", "管理保存的配置"), { patch("opened", true) }, leading = { SleepyAvatar(p.text("name", "木偶")) }, trailing = { SleepyBadge(p.text("badge", "已保存")) })
            "icon" -> SleepyGlyph(runCatching { SleepySymbol.valueOf(p.text("symbol", "Home")) }.getOrDefault(SleepySymbol.Home), Modifier.size(p.optInt("size", 32).coerceIn(16, 96).dp))
            "radio-group" -> SleepyRadioGroup(p.text("value", "default"), { patch("value", it) }, options(p), full, enabled)
            "filter-chips" -> SleepyFilterChips(strings(p, "value").toSet(), { patch("value", org.json.JSONArray(it.toList())) }, options(p), full, enabled)
            "stepper" -> SleepyStepper(p.optInt("value", 1), { patch("value", it) }, range = 0..99, enabled = enabled)
            "range-slider" -> Column(full) {
                val lower = p.optDouble("lower", 20.0).toFloat().coerceIn(0f, 100f)
                val upper = p.optDouble("upper", 80.0).toFloat().coerceIn(lower, 100f)
                Text("${lower.toInt()} — ${upper.toInt()}")
                SleepyRangeSlider(lower..upper, { patch("lower", it.start); patch("upper", it.endInclusive) }, full, enabled = enabled)
            }
            "date-picker" -> {
                val date = runCatching { LocalDate.parse(p.text("value", "2026-10-16")) }.getOrDefault(LocalDate.of(2026, 10, 16))
                SleepyButton(p.text("value", "选择日期"), { patch("open", true) }, enabled = enabled)
                SleepyDatePicker(p.optBoolean("open"), date, { patch("open", false) }, { patch("value", it.toString()) }, p.text("title", "选择日期"))
            }
            "time-picker" -> {
                val time = runCatching { LocalTime.parse(p.text("value", "09:30")) }.getOrDefault(LocalTime.of(9, 30))
                SleepyButton(p.text("value", "选择时间"), { patch("open", true) }, enabled = enabled)
                SleepyTimePicker(p.optBoolean("open"), time, { patch("open", false) }, { patch("value", it.toString()) }, p.text("title", "选择时间"), p.optBoolean("is24Hour", true))
            }
            "top-app-bar" -> SleepyTopAppBar(p.text("title", "我的任务"), full, onBack = if (p.optBoolean("back", true)) ({ patch("backClicks", p.optInt("backClicks") + 1) }) else null, onAction = { patch("actionClicks", p.optInt("actionClicks") + 1) })
            "bottom-navigation" -> SleepyBottomNavigation(p.text("value", "home"), { patch("value", it) }, options(p).mapIndexed { index, option -> SleepyNavigationItem(option.value, option.label, listOf(SleepySymbol.Home, SleepySymbol.Tasks, SleepySymbol.Settings)[index % 3], if (index == 1) p.optInt("badge", 3) else 0) }, full)
            "floating-action-button" -> SleepyFloatingActionButton(p.text("label", "新建任务"), { patch("clicks", p.optInt("clicks") + 1) }, extended = p.optBoolean("extended", true))
            "step-indicator" -> SleepyStepIndicator(p.optInt("current", 1), strings(p, "items"), full)
            "bottom-sheet" -> {
                SleepyButton("打开底部抽屉", { patch("open", true) })
                SleepyBottomSheet(p.optBoolean("open"), { patch("open", false) }, p.text("title", "任务详情")) {
                    Text(p.text("text", "调整配置后继续。"))
                    SleepyButton("完成", { patch("confirmed", true); patch("open", false) }, full)
                }
            }
            "action-sheet" -> {
                SleepyButton("选择操作", { patch("open", true) })
                SleepyActionSheet(p.optBoolean("open"), { patch("open", false) }, p.text("title", "文件操作"), actions(p), { patch("selected", it) })
            }
            "dropdown-menu" -> SleepyDropdownMenu(p.optBoolean("open"), { patch("open", false) }, actions(p), { patch("selected", it) }) {
                SleepyButton("更多操作", { patch("open", !p.optBoolean("open")) }, variant = ButtonVariant.Secondary)
            }
            "snackbar" -> if (!p.optBoolean("dismissed")) SleepySnackbar(p.text("text", "文件已移到回收站"), full, p.text("action", "撤销"), { patch("actionClicks", p.optInt("actionClicks") + 1) }, { patch("dismissed", true) }) else SleepyButton("再次显示", { patch("dismissed", false) })
            "circular-progress" -> SleepyCircularProgress(p.optDouble("progress", .6).toFloat(), Modifier.size(48.dp), p.optBoolean("indeterminate"))
            "skeleton" -> SleepySkeleton(full, p.optInt("lines", 3).coerceIn(1, 8), p.optBoolean("avatar", true))
            "divider" -> SleepyDivider(full, p.optString("label").ifBlank { null })
            "accordion" -> SleepyAccordion(p.text("title", "高级设置"), p.optBoolean("expanded", true), { patch("expanded", it) }, full) { Text(p.text("text", "展开后显示可复用的内容插槽。")) }
            "stat-card" -> SleepyStatCard(p.text("title", "本月完成"), p.text("value", "128"), full, p.text("description", "比上月增加 12 项"))
            "profile-card" -> SleepyProfileCard(p.text("name", "木偶"), p.text("description", "本地工作空间"), { patch("opened", true) }, full, p.text("action", "查看资料"))
            "file-item" -> SleepyFileItem(p.text("name", "采集路线.json"), p.text("metadata", "24 KB · 刚刚更新"), { patch("opened", true) }, full, p.optBoolean("selected"), { patch("selected", it) })
            "upload-card" -> if (!p.optBoolean("cancelled")) SleepyUploadCard(p.text("name", "任务附件.zip"), p.optDouble("progress", .45).toFloat(), { patch("cancelled", true) }, full, p.optString("error").ifBlank { null }, { patch("retries", p.optInt("retries") + 1); patch("error", "") }) else SleepyButton("重新上传", { patch("cancelled", false) })
        }
    }

    private fun options(p: JSONObject): List<SelectOption> {
        val input = p.optJSONArray("options") ?: return listOf(SelectOption("default", "默认配置"), SelectOption("custom", "自定义配置"))
        return (0 until input.length()).mapNotNull { index -> input.optJSONObject(index)?.let { SelectOption(it.optString("value"), it.optString("label")) } }
    }
    private fun JSONObject.text(key: String, fallback: String = ""): String = if (has(key) && !isNull(key)) optString(key) else fallback
    private fun strings(p: JSONObject, key: String): List<String> = p.optJSONArray(key)?.let { array -> (0 until array.length()).map { array.optString(it) } } ?: emptyList()
    private fun actions(p: JSONObject): List<SleepyMenuAction> = p.optJSONArray("actions")?.let { array ->
        (0 until array.length()).mapNotNull { index -> array.optJSONObject(index)?.let { SleepyMenuAction(it.optString("value"), it.optString("label"), it.optBoolean("enabled", true), it.optBoolean("destructive")) } }
    } ?: listOf(SleepyMenuAction("rename", "重命名"), SleepyMenuAction("share", "分享"), SleepyMenuAction("delete", "删除", destructive = true))
}
