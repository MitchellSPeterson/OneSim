package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.jsonString
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.BitmapStorage
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontStyle

/**
 * Shared state and tree serialization for the Compose guest (JVM wire host and Kotlin/Wasm browser target).
 */
object Guest {
    /** `isSystemInDarkTheme()`; updated by the host environment. */
    val dark = mutableStateOf(false)
    private val insetTopState = mutableStateOf(0f)
    private val insetBottomState = mutableStateOf(0f)

    var insetTop: Float
        get() = insetTopState.value
        set(value) {
            insetTopState.value = value
        }

    var insetBottom: Float
        get() = insetBottomState.value
        set(value) {
            insetBottomState.value = value
        }

    internal var logSink: (String) -> Unit = { println(it) }
    internal var uriSink: (String) -> Unit = {}
    internal var measureTextSink: (String) -> String = { error("Text measurement requires a running OneSim host") }
    internal var density: Float = 1f

    internal var content: (@Composable () -> Unit)? = null
    val root = LayoutNode("Box")
    var version = 0L
        internal set
    private var callbacks = CallbackTables()
    private var lastTree = ""
    private data class FontAsset(val signature: String, val data: String)
    data class FontResourceFace(val id: Int, val weight: Int, val italic: Boolean, val index: Int)
    private val fontAssets = mutableMapOf<Int, FontAsset>()
    private val fontFamilies = mutableMapOf<Int, List<FontResourceFace>>()
    private val providerFallbacks = mutableMapOf<Int, String>()
    private val fontResources = mutableMapOf<String, String>()
    private val images = mutableMapOf<Int, ImageBitmap>()

    fun registerImageResource(id: Int, width: Int, height: Int, hasAlpha: Boolean, argb: String): Int {
        val bytes = kotlin.io.encoding.Base64.decode(argb)
        val pixels = IntArray(width * height) { i ->
            val at = i * 4
            (0..3).fold(0) { pixel, channel -> (pixel shl 8) or (bytes[at + channel].toInt() and 255) }
        }
        images[id] = BitmapStorage(width, height, hasAlpha, pixels)
        return id
    }

    internal fun imageResource(id: Int): ImageBitmap =
        requireNotNull(images[id]) { "Invalid drawable resource ID: $id" }

    internal fun fontResourcesJson() = fontResources.entries.joinToString(
        prefix = "{", postfix = "}",
    ) { (family, faces) -> "${jsonString(family)}:$faces" }

    /** Called by the source-only build's generated R.font declarations. */
    fun registerFontResource(id: Int, signature: String, data: String): Int {
        fontAssets[id] = FontAsset(signature, data)
        return id
    }

    fun registerFontFamilyResource(id: Int, faces: List<FontResourceFace>): Int {
        fontFamilies[id] = faces
        return id
    }

    fun registerFontProviderFallback(id: Int, family: String): Int {
        providerFallbacks[id] = family
        return id
    }

    internal fun fontFamily(fonts: List<Font>): String {
        val faces = fonts.flatMap { font ->
            fontFamilies[font.resId] ?: listOfNotNull(
                fontAssets[font.resId]?.let { FontResourceFace(font.resId, font.weight.weight, font.style == FontStyle.Italic, 0) }
            )
        }
        if (faces.isEmpty()) return fonts.firstNotNullOfOrNull { providerFallbacks[it.resId] }?.let { "system:$it" } ?: "default"
        val assets = faces.map { fontAssets.getValue(it.id) }
        val name = faces.zip(assets).joinToString(prefix = "onesim:", separator = ":") { (face, asset) ->
            "${asset.signature}-${face.weight}-${face.italic}-${face.index}"
        }
        fontResources.getOrPut(name) {
            faces.zip(assets).joinToString(prefix = "[", postfix = "]") { (face, asset) ->
                """{"data":"${asset.data}","weight":${face.weight},"italic":${face.italic},"index":${face.index}}"""
            }
        }
        return name
    }

    fun setContent(c: @Composable () -> Unit) {
        content = c
    }

    internal fun log(line: String) = logSink(line)

    fun openUri(uri: String) {
        require(uri.substringBefore(':').lowercase() in setOf("http", "https", "mailto", "tel")) { "Unsupported URI scheme" }
        uriSink(uri)
    }

    internal fun reset() {
        content = null
        root.children.clear()
        version = 0L
        callbacks = CallbackTables()
        lastTree = ""
    }

    fun startActivity(activity: ComponentActivity): @Composable () -> Unit {
        activity.onCreate(null)
        activity.onStart()
        activity.onResume()
        return content ?: error("${activity::class.simpleName ?: "Activity"} did not call setContent { }")
    }

    /** Serialize the composed tree and refresh wire callback tables. Returns null if unchanged and `!force`. */
    fun serialize(force: Boolean): String? {
        val handlers = CallbackTables()
        val sb = StringBuilder()
        root.props = if (fontResources.isEmpty()) "" else "\"fontResources\":${fontResourcesJson()}"
        root.toJson("root", handlers, sb)
        val tree = sb.toString()
        // Callbacks can change while their wire IDs stay the same.
        callbacks = handlers
        if (!force && tree == lastTree) return null
        lastTree = tree
        version++
        return tree
    }

    fun dispatchAction(id: Long) {
        callbacks.actions[id]?.invoke()
    }

    fun dispatchTextInput(field: Long, text: String) {
        callbacks.inputs[field]?.invoke(text)
    }

    fun dispatchTextClick(id: Long, offset: Int) {
        callbacks.textClicks[id]?.invoke(offset)
    }

    fun dispatchTextLayouts(message: String) {
        val layouts = Regex("""\{\s*"id"\s*:\s*(\d+)\s*,\s*"geometry"\s*:\s*"([^"\\]*)"\s*\}""")
        for (layout in layouts.findAll(message)) {
            val id = layout.groupValues[1].toLongOrNull() ?: continue
            callbacks.textLayouts[id]?.invoke(layout.groupValues[2])
        }
    }
}

internal class CallbackTables {
    val actions = HashMap<Long, () -> Unit>()
    val inputs = HashMap<Long, (String) -> Unit>()
    val textClicks = HashMap<Long, (Int) -> Unit>()
    var textLayoutId = 0L
    var textProbeId = 1_000_000_000L
    val textLayouts = HashMap<Long, (String) -> Unit>()
}
