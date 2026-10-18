@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package dev.sleepydoll.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneOffset

@Composable
fun SleepyRadioGroup(value: String, onValueChange: (String) -> Unit, items: List<SelectOption>, modifier: Modifier = Modifier, enabled: Boolean = true) {
    Column(modifier.selectableGroup()) {
        items.forEach { item ->
            Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(item.value == value, enabled = enabled, role = Role.RadioButton, onClick = { onValueChange(item.value) }).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                RadioButton(item.value == value, null, enabled = enabled)
                Spacer(Modifier.width(12.dp))
                Text(item.label, color = if (enabled) SleepyTheme.colors.text else SleepyTheme.colors.muted)
            }
        }
    }
}

@Composable
fun SleepyFilterChips(value: Set<String>, onValueChange: (Set<String>) -> Unit, items: List<SelectOption>, modifier: Modifier = Modifier, enabled: Boolean = true) {
    FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        items.forEach { item ->
            FilterChip(selected = item.value in value, onClick = { onValueChange(if (item.value in value) value - item.value else value + item.value) }, label = { Text(item.label) }, enabled = enabled,
                colors = FilterChipDefaults.filterChipColors(selectedContainerColor = SleepyTheme.colors.selected, selectedLabelColor = SleepyTheme.colors.text))
        }
    }
}

@Composable
fun SleepyStepper(value: Int, onValueChange: (Int) -> Unit, modifier: Modifier = Modifier, range: IntRange = 0..99, step: Int = 1, enabled: Boolean = true, label: String = "数量") {
    require(!range.isEmpty() && step > 0)
    val current = value.coerceIn(range)
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        SleepyIconButton("减少$label", { onValueChange((current.toLong() - step).coerceAtLeast(range.first.toLong()).toInt()) }, enabled = enabled && current > range.first) { SleepyGlyph(SleepySymbol.Minus) }
        Text(current.toString(), style = MaterialTheme.typography.titleMedium)
        SleepyIconButton("增加$label", { onValueChange((current.toLong() + step).coerceAtMost(range.last.toLong()).toInt()) }, enabled = enabled && current < range.last) { SleepyGlyph(SleepySymbol.Add) }
    }
}

@Composable
fun SleepyRangeSlider(value: ClosedFloatingPointRange<Float>, onValueChange: (ClosedFloatingPointRange<Float>) -> Unit, modifier: Modifier = Modifier, range: ClosedFloatingPointRange<Float> = 0f..100f, enabled: Boolean = true) {
    require(range.start.isFinite() && range.endInclusive.isFinite() && range.start < range.endInclusive)
    val start = value.start.coerceIn(range.start, range.endInclusive)
    val end = value.endInclusive.coerceIn(start, range.endInclusive)
    RangeSlider(value = start..end, onValueChange = onValueChange, modifier = modifier, valueRange = range, enabled = enabled,
        colors = SliderDefaults.colors(thumbColor = SleepyTheme.colors.solid, activeTrackColor = SleepyTheme.colors.solid, inactiveTrackColor = SleepyTheme.colors.selected))
}

/** Dates are calendar dates; Material's UTC epoch conversion stays inside this adapter. */
@Composable
fun SleepyDatePicker(open: Boolean, value: LocalDate?, onDismiss: () -> Unit, onConfirm: (LocalDate) -> Unit, title: String = "选择日期") {
    if (!open) return
    key(value) {
        val state = rememberDatePickerState(initialSelectedDateMillis = value?.atStartOfDay()?.toInstant(ZoneOffset.UTC)?.toEpochMilli())
        val colors = DatePickerDefaults.colors(containerColor = SleepyTheme.colors.surface, selectedDayContainerColor = SleepyTheme.colors.solid, selectedDayContentColor = SleepyTheme.colors.inverse, selectedYearContainerColor = SleepyTheme.colors.solid, selectedYearContentColor = SleepyTheme.colors.inverse, todayDateBorderColor = SleepyTheme.colors.solid, todayContentColor = SleepyTheme.colors.text)
        val maxHeight = (LocalConfiguration.current.screenHeightDp - 80).coerceAtLeast(160).dp
        DatePickerDialog(onDismissRequest = onDismiss, colors = colors, tonalElevation = 0.dp, confirmButton = { SleepyButton("确定", { state.selectedDateMillis?.let { onConfirm(Instant.ofEpochMilli(it).atOffset(ZoneOffset.UTC).toLocalDate()); onDismiss() } }, enabled = state.selectedDateMillis != null) }, dismissButton = { SleepyButton("取消", onDismiss, variant = ButtonVariant.Text) }) {
            Column(Modifier.heightIn(max = maxHeight).verticalScroll(rememberScrollState())) {
                Text(title, Modifier.padding(start = 24.dp, top = 16.dp, bottom = 8.dp), style = MaterialTheme.typography.titleMedium)
                DatePicker(state = state, title = null, headline = null, showModeToggle = false, colors = colors)
            }
        }
    }
}

@Composable
fun SleepyTimePicker(open: Boolean, value: LocalTime, onDismiss: () -> Unit, onConfirm: (LocalTime) -> Unit, title: String = "选择时间", is24Hour: Boolean = true) {
    if (!open) return
    key(value, is24Hour) {
        val state = rememberTimePickerState(initialHour = value.hour, initialMinute = value.minute, is24Hour = is24Hour)
        AlertDialog(onDismissRequest = onDismiss, title = { Text(title) }, text = { TimeInput(state, colors = TimePickerDefaults.colors(timeSelectorSelectedContainerColor = SleepyTheme.colors.selected, timeSelectorUnselectedContainerColor = SleepyTheme.colors.subtle, timeSelectorSelectedContentColor = SleepyTheme.colors.text, timeSelectorUnselectedContentColor = SleepyTheme.colors.secondary)) }, confirmButton = { SleepyButton("确定", { onConfirm(LocalTime.of(state.hour, state.minute)); onDismiss() }) }, dismissButton = { SleepyButton("取消", onDismiss, variant = ButtonVariant.Text) }, containerColor = SleepyTheme.colors.surface)
    }
}
