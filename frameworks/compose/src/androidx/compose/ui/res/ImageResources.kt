package androidx.compose.ui.res

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.ImageBitmap
import onesim.compose.Guest

@Composable
fun ImageBitmap.Companion.imageResource(id: Int): ImageBitmap = Guest.imageResource(id)
