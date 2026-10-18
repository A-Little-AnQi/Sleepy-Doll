package dev.sleepydoll.preview

import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.graphics.Bitmap
import android.util.Base64
import android.util.Log
import android.view.PixelCopy
import android.view.ViewTreeObserver
import android.view.inspector.WindowInspector
import android.view.inputmethod.EditorInfo
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import dev.sleepydoll.ui.*
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import kotlin.coroutines.resume

class PreviewActivity : ComponentActivity() {
    private var session = 0
    private var blur by mutableIntStateOf(0)
    private var committedRevision = -1L
    private var removingSplash = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= 31) {
            splashScreen.setOnExitAnimationListener { splash ->
                removingSplash = true
                splash.remove()
                window.decorView.postOnAnimation { removingSplash = false }
            }
        }
        val token = intent.getStringExtra("token")
        if (token == null || !token.matches(Regex("[a-fA-F0-9]{64}"))) { finish(); return }
        session = intent.getIntExtra("session", 0)
        Log.d("SleepyPreview", "activity created session=$session")
        PreviewSession.start(token)
        PreviewSession.attach(session, this)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply { hide(WindowInsetsCompat.Type.systemBars()); systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE }
        setContent {
            val model = PreviewSession.models[session]
            LaunchedEffect(model?.revision) { Log.d("SleepyPreview", "compose session=$session model=${model?.id}") }
            val focus = LocalFocusManager.current
            LaunchedEffect(blur) { if (blur > 0) focus.clearFocus(force = true) }
            if (model == null) {
                SleepyTheme { Surface(Modifier.fillMaxSize()) { Box(contentAlignment = Alignment.Center) { Text("Sleepy Doll") } } }
            } else {
                val dark = model.theme.optString("mode") == "dark"
                val defaults = if (dark) SleepyColors.Dark else SleepyColors.Light
                val colors = defaults.copy(surface = model.theme.color("surface", defaults.surface), text = model.theme.color("text", defaults.text), muted = model.theme.color("muted", defaults.muted), border = model.theme.color("border", defaults.border), accent = model.theme.color("accent", defaults.accent))
                val design = SleepyDesign(radius = model.theme.optDouble("radius", 12.0).toFloat().coerceIn(0f, 24f), fontSize = model.theme.optDouble("fontSize", 16.0).toFloat().coerceIn(12f, 24f), borderWidth = model.theme.optDouble("borderWidth", 1.0).toFloat().coerceIn(0f, 6f), motionMillis = model.theme.optInt("motion", 240).coerceIn(0, 600), shadow = model.theme.optString("shadow", "soft") != "none")
                SleepyTheme(dark, colors, design) {
                    Surface(Modifier.fillMaxSize(), color = colors.surface) {
                        Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                            Box(Modifier.widthIn(max = 600.dp).fillMaxWidth(), contentAlignment = Alignment.Center) { NativeComponents.Render(model) }
                        }
                    }
                }
                LaunchedEffect(model.revision) {
                    PreviewSession.moving(session)
                    awaitCommittedFrame()
                    while (removingSplash) awaitCommittedFrame()
                    committedRevision = model.revision
                    PreviewSession.painted(session)
                    delay(design.motionMillis.toLong() + 200)
                    if (model.isAnimated()) {
                        while (true) { PreviewSession.moving(session); delay(400) }
                    } else PreviewSession.still(session)
                }
            }
        }
    }

    fun commitText(text: String) {
        val focus = currentFocus ?: window.decorView.findFocus() ?: return
        val connection = focus.onCreateInputConnection(EditorInfo()) ?: return
        connection.beginBatchEdit()
        connection.commitText(text, 1)
        connection.endBatchEdit()
    }
    fun clearInputFocus() { blur++ }
    fun isFrameReady(revision: Long): Boolean = committedRevision == revision && !removingSplash

    // Frame-clock ticks alone do not mean the new content reached the surface.
    private suspend fun awaitCommittedFrame() {
        val view = window.decorView
        suspendCancellableCoroutine<Unit> { continuation ->
            val observer = view.viewTreeObserver
            if (Build.VERSION.SDK_INT >= 29 && view.isHardwareAccelerated) {
                val callback = Runnable { if (continuation.isActive) continuation.resume(Unit) }
                observer.registerFrameCommitCallback(callback)
                continuation.invokeOnCancellation {
                    if (observer.isAlive) observer.unregisterFrameCommitCallback(callback)
                }
            } else {
                val callback = object : ViewTreeObserver.OnDrawListener {
                    override fun onDraw() {
                        view.post {
                            if (observer.isAlive) observer.removeOnDrawListener(this)
                            if (continuation.isActive) continuation.resume(Unit)
                        }
                    }
                }
                observer.addOnDrawListener(callback)
                continuation.invokeOnCancellation {
                    view.post { if (observer.isAlive) observer.removeOnDrawListener(callback) }
                }
            }
            view.postInvalidateOnAnimation()
        }
    }

    fun capture() {
        val model = PreviewSession.models[session] ?: return
        if (!isFrameReady(model.revision)) return
        if (model.isAnimated()) { PreviewSession.moving(session); return }
        val view = window.decorView
        val width = view.width
        val height = view.height
        if (width <= 0 || height <= 0 || isFinishing || isDestroyed) return
        if (android.os.Build.VERSION.SDK_INT >= 29 && WindowInspector.getGlobalWindowViews().count { it.isShown && it.display?.displayId == view.display?.displayId } > 1) {
            PreviewSession.moving(session)
            return
        }
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        runCatching {
            PixelCopy.request(window, bitmap, { result ->
                try {
                    if (result == PixelCopy.SUCCESS) {
                        val bytes = ByteArrayOutputStream()
                        bitmap.compress(Bitmap.CompressFormat.PNG, 100, bytes)
                        PreviewSession.image(session, model.revision, width, height, Base64.encodeToString(bytes.toByteArray(), Base64.NO_WRAP))
                    }
                } finally { bitmap.recycle() }
            }, Handler(Looper.getMainLooper()))
        }.onFailure { bitmap.recycle() }
    }
    override fun onDestroy() { PreviewSession.detach(session, this); super.onDestroy() }
    private fun PreviewModel.isAnimated(): Boolean = props.optBoolean("indeterminate") || props.optBoolean("loading") || props.optBoolean("busy")
    private fun JSONObject.color(key: String, fallback: Color): Color = runCatching { Color(android.graphics.Color.parseColor(getString(key))) }.getOrDefault(fallback)
}
