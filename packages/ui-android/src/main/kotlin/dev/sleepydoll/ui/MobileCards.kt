package dev.sleepydoll.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@Composable
fun SleepyStatCard(title: String, value: String, modifier: Modifier = Modifier, supportingText: String? = null) {
    OutlinedCard(modifier, shape = MaterialTheme.shapes.large, colors = CardDefaults.outlinedCardColors(containerColor = SleepyTheme.colors.surface), border = BorderStroke(SleepyTheme.design.borderWidth.dp, SleepyTheme.colors.border)) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.bodyMedium, color = SleepyTheme.colors.secondary)
            Text(value, style = MaterialTheme.typography.titleLarge)
            if (!supportingText.isNullOrBlank()) Text(supportingText, style = MaterialTheme.typography.bodySmall, color = SleepyTheme.colors.muted)
        }
    }
}

@Composable
fun SleepyProfileCard(name: String, description: String, onAction: () -> Unit, modifier: Modifier = Modifier, actionLabel: String = "查看资料") {
    OutlinedCard(modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, colors = CardDefaults.outlinedCardColors(containerColor = SleepyTheme.colors.surface), border = BorderStroke(SleepyTheme.design.borderWidth.dp, SleepyTheme.colors.border)) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                SleepyAvatar(name)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(name, style = MaterialTheme.typography.titleMedium)
                    Text(description, style = MaterialTheme.typography.bodySmall, color = SleepyTheme.colors.muted)
                }
            }
            SleepyButton(actionLabel, onAction, variant = ButtonVariant.Secondary)
        }
    }
}

@Composable
fun SleepyFileItem(name: String, metadata: String, onOpen: () -> Unit, modifier: Modifier = Modifier, selected: Boolean = false, onSelectedChange: ((Boolean) -> Unit)? = null) {
    Surface(modifier.fillMaxWidth(), color = if (selected) SleepyTheme.colors.subtle else SleepyTheme.colors.surface, shape = MaterialTheme.shapes.medium) {
        SleepyListItem(name, metadata, onOpen, leading = { SleepyGlyph(SleepySymbol.File) }, trailing = { onSelectedChange?.let { SleepyCheckbox(selected, it, "选择$name", labelVisible = false) } })
    }
}

@Composable
fun SleepyUploadCard(name: String, progress: Float, onCancel: () -> Unit, modifier: Modifier = Modifier, error: String? = null, onRetry: () -> Unit = {}) {
    OutlinedCard(modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, colors = CardDefaults.outlinedCardColors(containerColor = SleepyTheme.colors.surface), border = BorderStroke(SleepyTheme.design.borderWidth.dp, SleepyTheme.colors.border)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                SleepyGlyph(SleepySymbol.Upload)
                Text(name, Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
                SleepyIconButton("取消上传", onCancel) { SleepyGlyph(SleepySymbol.Close) }
            }
            if (error.isNullOrBlank()) {
                SleepyProgress(progress, trackColor = SleepyTheme.colors.selected)
                Text(if (progress >= 1f) "上传完成" else "已上传 ${(progress.coerceIn(0f, 1f) * 100).toInt()}%", style = MaterialTheme.typography.bodySmall, color = SleepyTheme.colors.muted)
            } else SleepyNotice(error, actionLabel = "重试", onAction = onRetry)
        }
    }
}
