package dev.sleepydoll.preview

import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.runtime.mutableStateMapOf
import org.json.JSONObject
import java.io.BufferedWriter
import java.io.Closeable
import java.io.InputStreamReader
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.MessageDigest
import java.util.concurrent.Executors

data class PreviewModel(val session: Int, val id: String, val props: JSONObject, val theme: JSONObject, val revision: Long, val wireRevision: String, val playback: Int)

/** Frame 协议 4 的本机桥；只有预览宿主拥有端口、会话与视频相关依赖。 */
object PreviewSession {
    val models = mutableStateMapOf<Int, PreviewModel>()
    private val main = Handler(Looper.getMainLooper())
    private val workers = Executors.newCachedThreadPool { runnable -> Thread(runnable, "sleepy-preview").apply { isDaemon = true } }
    private val outgoing = Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "sleepy-preview-events").apply { isDaemon = true } }
    private val outputs = mutableSetOf<BufferedWriter>()
    private val activities = mutableMapOf<Int, PreviewActivity>()
    private var secret: String? = null
    private var server: ServerSocket? = null
    private var revision = 0L

    @Synchronized
    fun start(token: String) {
        require(token.matches(Regex("[a-fA-F0-9]{64}"))) { "Invalid preview token" }
        if (secret != null && secret != token) {
            server?.close()
            synchronized(outputs) { outputs.forEach { runCatching { it.close() } }; outputs.clear() }
            models.clear()
            server = null
        }
        secret = token
        if (server != null) return
        val listener = ServerSocket(7831, 8, InetAddress.getByName("127.0.0.1"))
        server = listener
        workers.execute {
            while (!listener.isClosed) {
                val socket = runCatching { listener.accept() }.getOrNull() ?: break
                workers.execute { runCatching { serve(socket) } }
            }
        }
    }

    private fun serve(socket: Socket) {
        socket.use {
            socket.soTimeout = 5_000
            val input = InputStreamReader(socket.getInputStream(), Charsets.UTF_8)
            val token = readLine(input, 128) ?: return
            if (!MessageDigest.isEqual(token.toByteArray(), secret.orEmpty().toByteArray())) return
            socket.soTimeout = 0
            val output = socket.getOutputStream().bufferedWriter(Charsets.UTF_8)
            synchronized(outputs) { outputs.add(output) }
            try {
                write(output, JSONObject().put("type", "ready").put("protocol", 4))
                while (true) {
                    val line = readLine(input, 1_048_576) ?: break
                    val command = runCatching { JSONObject(line) }.getOrNull() ?: break
                    main.post { handle(command) }
                }
            } finally { synchronized(outputs) { outputs.remove(output) } }
        }
    }

    private fun readLine(reader: InputStreamReader, limit: Int): String? {
        val line = StringBuilder()
        while (true) {
            val value = reader.read()
            if (value < 0) return if (line.isEmpty()) null else line.toString()
            if (value == 10) return line.toString().trimEnd('\r')
            if (line.length >= limit) return null
            line.append(value.toChar())
        }
    }

    private fun handle(command: JSONObject) {
        val session = command.optInt("session")
        Log.d("SleepyPreview", "command=${command.optString("type")} session=$session id=${command.optString("id")}")
        when (command.optString("type")) {
            "render" -> {
                val id = command.optString("id")
                if (id !in NativeComponents.ids) { Log.w("SleepyPreview", "Unknown component: $id"); return }
                models[session] = PreviewModel(session, id, command.optJSONObject("props") ?: JSONObject(), command.optJSONObject("theme") ?: JSONObject(), ++revision, command.optString("revision"), command.optInt("playback"))
                Log.d("SleepyPreview", "model stored session=$session activities=${activities.keys}")
            }
            "input" -> activities[session]?.commitText(command.optString("text").take(10_000))
            "blur" -> activities[session]?.clearInputFocus()
            "capture" -> { painted(session); activities[session]?.capture() }
            "release" -> { models.remove(session); activities.remove(session)?.finish() }
        }
    }

    fun attach(session: Int, activity: PreviewActivity) { activities[session] = activity }
    fun detach(session: Int, activity: PreviewActivity) { if (activities[session] === activity) activities.remove(session) }

    fun patch(session: Int, key: String, value: Any?) {
        val model = models[session] ?: return
        val props = JSONObject(model.props.toString()).put(key, value ?: JSONObject.NULL)
        models[session] = model.copy(props = props, revision = ++revision)
        emit(JSONObject().put("type", "patch").put("session", session).put("id", model.id).put("revision", model.wireRevision).put("key", key).put("value", value ?: JSONObject.NULL))
    }

    fun moving(session: Int) { event("moving", session) }
    fun painted(session: Int) {
        val expected = models[session]?.revision ?: return
        if (activities[session]?.isFrameReady(expected) != true) return
        event("painted", session)
        main.postDelayed({ if (models[session]?.revision == expected) activities[session]?.capture() }, 250)
    }
    fun still(session: Int) { activities[session]?.capture() }
    fun image(session: Int, revision: Long, width: Int, height: Int, image: String) {
        val model = models[session] ?: return
        if (model.revision != revision) return
        emit(JSONObject().put("type", "still").put("session", session).put("id", model.id).put("revision", model.wireRevision).put("playback", model.playback).put("width", width).put("height", height).put("image", image))
    }
    private fun event(type: String, session: Int) {
        val model = models[session] ?: return
        emit(JSONObject().put("type", type).put("session", session).put("id", model.id).put("revision", model.wireRevision))
    }
    private fun emit(event: JSONObject) {
        // Compose callbacks run on the main thread; socket writes must not.
        // One writer also preserves moving/painted/still and patch ordering.
        outgoing.execute {
            synchronized(outputs) {
                outputs.removeAll { output ->
                    runCatching { write(output, event) }.onFailure {
                        Log.w("SleepyPreview", "Could not send preview event", it)
                    }.isFailure
                }
            }
        }
    }
    private fun write(output: BufferedWriter, event: JSONObject) {
        synchronized(output) { output.write(event.toString()); output.newLine(); output.flush() }
    }
}
