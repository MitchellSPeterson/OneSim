package androidx.compose.ui.platform

import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.HostFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import onesim.compose.Guest

val LocalDensity = compositionLocalOf { Density(Guest.density) }
val LocalLayoutDirection = compositionLocalOf { LayoutDirection.Ltr }
val LocalFontFamilyResolver = compositionLocalOf<FontFamily.Resolver> { HostFontFamilyResolver }
