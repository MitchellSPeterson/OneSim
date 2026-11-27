package com.example.navigation

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** A 24 dp stand-in for an icon (the same code runs on OneSim and the device): a filled circle or rounded square. */
@Composable
fun Glyph(round: Boolean = true) {
    Box(Modifier.size(24.dp)) {
        Box(Modifier.size(18.dp).let { if (round) it.background(LocalContentColor.current, CircleShape) else it.background(LocalContentColor.current, RoundedCornerShape(4.dp)) }.align(androidx.compose.ui.Alignment.Center))
    }
}

/** A 48 dp touch target around a glyph, like an IconButton. */
@Composable
fun GlyphButton(round: Boolean = true) {
    Box(Modifier.size(48.dp), contentAlignment = androidx.compose.ui.Alignment.Center) { Glyph(round) }
}
