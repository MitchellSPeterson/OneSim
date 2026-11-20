package androidx.compose.ui.graphics.colorspace

class ColorSpace internal constructor(val name: String) {
    override fun toString() = name
}

object ColorSpaces {
    val Srgb = ColorSpace("sRGB IEC61966-2.1")
}
