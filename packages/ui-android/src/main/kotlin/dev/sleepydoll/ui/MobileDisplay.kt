package dev.sleepydoll.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
fun SleepyCircularProgress(progress: Float, modifier: Modifier = Modifier, indeterminate: Boolean = false) {
    if (indeterminate) CircularProgressIndicator(modifier = modifier, color = SleepyTheme.colors.solid, trackColor = SleepyTheme.colors.subtle)
    else CircularProgressIndicator(progress = { progress.coerceIn(0f, 1f) }, modifier = modifier, color = SleepyTheme.colors.solid, trackColor = SleepyTheme.colors.subtle)
}

@Composable
fun SleepySkeleton(modifier: Modifier = Modifier, lines: Int = 3, avatar: Boolean = true) {
    require(lines in 1..8)
    Row(modifier.fillMaxWidth().semantics { contentDescription = "正在加载内容" }, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        if (avatar) Box(Modifier.size(44.dp).clip(MaterialTheme.shapes.large).background(SleepyTheme.colors.selected))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            repeat(lines) { index -> Box(Modifier.fillMaxWidth(if (index == lines - 1) .65f else 1f).height(14.dp).clip(RoundedCornerShape(4.dp)).background(SleepyTheme.colors.subtle)) }
        }
    }
}

@Composable
fun SleepyDivider(modifier: Modifier = Modifier, label: String? = null) {
    if (label.isNullOrBlank()) HorizontalDivider(modifier, thickness = SleepyTheme.design.borderWidth.dp, color = SleepyTheme.colors.border)
    else Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        HorizontalDivider(Modifier.weight(1f), thickness = SleepyTheme.design.borderWidth.dp, color = SleepyTheme.colors.border)
        Text(label, style = MaterialTheme.typography.labelMedium, color = SleepyTheme.colors.muted)
        HorizontalDivider(Modifier.weight(1f), thickness = SleepyTheme.design.borderWidth.dp, color = SleepyTheme.colors.border)
    }
}

@Composable
fun SleepyAccordion(title: String, expanded: Boolean, onExpandedChange: (Boolean) -> Unit, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Surface(modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = SleepyTheme.colors.surface, border = BorderStroke(SleepyTheme.design.borderWidth.dp, SleepyTheme.colors.border)) {
        Column {
            SleepyListItem(title, if (expanded) "收起内容" else "展开内容", { onExpandedChange(!expanded) }, trailing = { SleepyGlyph(if (expanded) SleepySymbol.Minus else SleepySymbol.Add) })
            if (expanded) Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
        }
    }
}
