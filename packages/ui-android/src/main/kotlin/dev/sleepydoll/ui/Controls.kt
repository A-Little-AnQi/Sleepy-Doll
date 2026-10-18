@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package dev.sleepydoll.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp

enum class ButtonVariant { Primary, Secondary, Text }

@Composable
fun SleepyButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, loading: Boolean = false, variant: ButtonVariant = ButtonVariant.Primary) {
    val design = SleepyTheme.design
    val content: @Composable RowScope.() -> Unit = {
        if (loading) { CircularProgressIndicator(Modifier.size(16.dp), color = LocalContentColor.current, strokeWidth = 2.dp); Spacer(Modifier.width(8.dp)) }
        Text(label)
    }
    val shape = MaterialTheme.shapes.medium
    val size = modifier.heightIn(min = 48.dp)
    when (variant) {
        ButtonVariant.Primary -> Button(onClick, size, enabled && !loading, shape = shape, content = content)
        ButtonVariant.Secondary -> OutlinedButton(onClick, size, enabled && !loading, shape = shape, border = BorderStroke(design.borderWidth.dp, SleepyTheme.colors.border), content = content)
        ButtonVariant.Text -> TextButton(onClick, size, enabled && !loading, shape = shape, content = content)
    }
}

@Composable
fun SleepyIconButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, content: @Composable () -> Unit) {
    IconButton(onClick, modifier.sizeIn(minWidth = 48.dp, minHeight = 48.dp).semantics { contentDescription = label }, enabled, content = content)
}

@Composable
fun SleepyTextField(value: String, onValueChange: (String) -> Unit, label: String, modifier: Modifier = Modifier, placeholder: String = "", enabled: Boolean = true, error: String? = null, multiline: Boolean = false, password: Boolean = false) {
    OutlinedTextField(value, onValueChange, modifier, enabled = enabled, label = { Text(label) }, placeholder = { Text(placeholder) }, isError = error != null, supportingText = error?.let { { Text(it) } }, singleLine = !multiline, minLines = if (multiline) 3 else 1, maxLines = if (multiline) 8 else 1, visualTransformation = if (password) PasswordVisualTransformation() else VisualTransformation.None, shape = MaterialTheme.shapes.medium)
}

@Composable
fun SleepySearchField(value: String, onValueChange: (String) -> Unit, modifier: Modifier = Modifier, placeholder: String = "搜索", enabled: Boolean = true) {
    OutlinedTextField(value, onValueChange, modifier, enabled = enabled, singleLine = true, placeholder = { Text(placeholder) }, leadingIcon = { Text("⌕", style = MaterialTheme.typography.titleLarge) }, shape = MaterialTheme.shapes.medium)
}

@Composable
fun SleepySwitch(checked: Boolean, onCheckedChange: (Boolean) -> Unit, label: String, modifier: Modifier = Modifier, enabled: Boolean = true) {
    Switch(checked, onCheckedChange, modifier.semantics { contentDescription = label }, enabled = enabled)
}

@Composable
fun SleepyCheckbox(checked: Boolean, onCheckedChange: (Boolean) -> Unit, label: String, modifier: Modifier = Modifier, enabled: Boolean = true, labelVisible: Boolean = true) {
    Row(modifier.heightIn(min = 48.dp).toggleable(value = checked, enabled = enabled, role = Role.Checkbox, onValueChange = onCheckedChange).semantics { if (!labelVisible) contentDescription = label }.padding(end = if (labelVisible) 12.dp else 0.dp), verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked, null, enabled = enabled)
        if (labelVisible) Text(label, style = MaterialTheme.typography.bodyMedium, color = if (enabled) SleepyTheme.colors.text else SleepyTheme.colors.muted)
    }
}

@Composable
fun SleepySlider(value: Float, onValueChange: (Float) -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, range: ClosedFloatingPointRange<Float> = 0f..100f) {
    Slider(value, onValueChange, modifier, enabled = enabled, valueRange = range)
}

data class SelectOption(val value: String, val label: String)

@Composable
fun SleepySelect(value: String, onValueChange: (String) -> Unit, label: String, options: List<SelectOption>, modifier: Modifier = Modifier, enabled: Boolean = true) {
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded, { if (enabled) expanded = it }, modifier) {
        OutlinedTextField(options.firstOrNull { it.value == value }?.label ?: "", {}, Modifier.menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled).fillMaxWidth(), enabled = enabled, readOnly = true, singleLine = true, label = { Text(label) }, trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) }, shape = MaterialTheme.shapes.medium)
        ExposedDropdownMenu(expanded && enabled, { expanded = false }) {
            options.forEach { option -> DropdownMenuItem({ Text(option.label) }, { onValueChange(option.value); expanded = false }) }
        }
    }
}

@Composable
fun SleepyTabs(value: String, onValueChange: (String) -> Unit, items: List<SelectOption>, modifier: Modifier = Modifier) {
    Row(modifier.clip(MaterialTheme.shapes.medium).background(SleepyTheme.colors.subtle).padding(4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        items.forEach { item ->
            TextButton({ onValueChange(item.value) }, Modifier.weight(1f), shape = MaterialTheme.shapes.small, colors = ButtonDefaults.textButtonColors(containerColor = if (item.value == value) SleepyTheme.colors.selected else Color.Transparent)) { Text(item.label, maxLines = 1) }
        }
    }
}

@Composable
fun SleepyBadge(text: String, modifier: Modifier = Modifier, active: Boolean = false) {
    Surface(modifier, shape = MaterialTheme.shapes.small, color = if (active) SleepyTheme.colors.selected else SleepyTheme.colors.subtle, border = BorderStroke(SleepyTheme.design.borderWidth.dp, SleepyTheme.colors.border)) { Text(text, Modifier.padding(horizontal = 8.dp, vertical = 4.dp), style = MaterialTheme.typography.labelMedium, color = SleepyTheme.colors.secondary) }
}

@Composable
fun SleepyAvatar(name: String, modifier: Modifier = Modifier) {
    Box(modifier.size(44.dp).clip(CircleShape).background(SleepyTheme.colors.selected), contentAlignment = Alignment.Center) { Text(name.take(2), color = SleepyTheme.colors.text, style = MaterialTheme.typography.titleMedium) }
}

@Composable
fun SleepyProgress(progress: Float, modifier: Modifier = Modifier, indeterminate: Boolean = false, trackColor: Color = ProgressIndicatorDefaults.linearTrackColor) {
    if (indeterminate) LinearProgressIndicator(modifier = modifier.fillMaxWidth(), trackColor = trackColor) else LinearProgressIndicator(progress = { progress.coerceIn(0f, 1f) }, modifier = modifier.fillMaxWidth(), trackColor = trackColor)
}
