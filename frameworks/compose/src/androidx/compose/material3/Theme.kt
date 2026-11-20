package androidx.compose.material3

import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors

import androidx.compose.foundation.shape.CornerBasedShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Material 3 color roles (baseline tokens, `ColorLightTokens` / `ColorDarkTokens`). */
@Immutable
data class ColorScheme(
    val primary: Color, val onPrimary: Color, val primaryContainer: Color, val onPrimaryContainer: Color, val inversePrimary: Color,
    val secondary: Color, val onSecondary: Color, val secondaryContainer: Color, val onSecondaryContainer: Color,
    val tertiary: Color, val onTertiary: Color, val tertiaryContainer: Color, val onTertiaryContainer: Color,
    val background: Color, val onBackground: Color, val surface: Color, val onSurface: Color,
    val surfaceVariant: Color, val onSurfaceVariant: Color, val surfaceTint: Color,
    val inverseSurface: Color, val inverseOnSurface: Color,
    val error: Color, val onError: Color, val errorContainer: Color, val onErrorContainer: Color,
    val outline: Color, val outlineVariant: Color, val scrim: Color,
    val surfaceBright: Color, val surfaceContainer: Color, val surfaceContainerHigh: Color, val surfaceContainerHighest: Color,
    val surfaceContainerLow: Color, val surfaceContainerLowest: Color, val surfaceDim: Color,
)

fun lightColorScheme(
    primary: Color = Color(0xFF6750A4), onPrimary: Color = Color(0xFFFFFFFF), primaryContainer: Color = Color(0xFFEADDFF),
    onPrimaryContainer: Color = Color(0xFF21005D), inversePrimary: Color = Color(0xFFD0BCFF),
    secondary: Color = Color(0xFF625B71), onSecondary: Color = Color(0xFFFFFFFF), secondaryContainer: Color = Color(0xFFE8DEF8),
    onSecondaryContainer: Color = Color(0xFF1D192B),
    tertiary: Color = Color(0xFF7D5260), onTertiary: Color = Color(0xFFFFFFFF), tertiaryContainer: Color = Color(0xFFFFD8E4),
    onTertiaryContainer: Color = Color(0xFF31111D),
    background: Color = Color(0xFFFEF7FF), onBackground: Color = Color(0xFF1D1B20), surface: Color = Color(0xFFFEF7FF),
    onSurface: Color = Color(0xFF1D1B20), surfaceVariant: Color = Color(0xFFE7E0EC), onSurfaceVariant: Color = Color(0xFF49454F),
    surfaceTint: Color = primary, inverseSurface: Color = Color(0xFF322F35), inverseOnSurface: Color = Color(0xFFF5EFF7),
    error: Color = Color(0xFFB3261E), onError: Color = Color(0xFFFFFFFF), errorContainer: Color = Color(0xFFF9DEDC),
    onErrorContainer: Color = Color(0xFF410E0B), outline: Color = Color(0xFF79747E), outlineVariant: Color = Color(0xFFCAC4D0),
    scrim: Color = Color(0xFF000000), surfaceBright: Color = Color(0xFFFEF7FF), surfaceContainer: Color = Color(0xFFF3EDF7),
    surfaceContainerHigh: Color = Color(0xFFECE6F0), surfaceContainerHighest: Color = Color(0xFFE6E0E9),
    surfaceContainerLow: Color = Color(0xFFF7F2FA), surfaceContainerLowest: Color = Color(0xFFFFFFFF), surfaceDim: Color = Color(0xFFDED8E1),
) = ColorScheme(
    primary, onPrimary, primaryContainer, onPrimaryContainer, inversePrimary, secondary, onSecondary, secondaryContainer,
    onSecondaryContainer, tertiary, onTertiary, tertiaryContainer, onTertiaryContainer, background, onBackground, surface, onSurface,
    surfaceVariant, onSurfaceVariant, surfaceTint, inverseSurface, inverseOnSurface, error, onError, errorContainer, onErrorContainer,
    outline, outlineVariant, scrim, surfaceBright, surfaceContainer, surfaceContainerHigh, surfaceContainerHighest, surfaceContainerLow,
    surfaceContainerLowest, surfaceDim,
)

fun darkColorScheme(
    primary: Color = Color(0xFFD0BCFF), onPrimary: Color = Color(0xFF381E72), primaryContainer: Color = Color(0xFF4F378B),
    onPrimaryContainer: Color = Color(0xFFEADDFF), inversePrimary: Color = Color(0xFF6750A4),
    secondary: Color = Color(0xFFCCC2DC), onSecondary: Color = Color(0xFF332D41), secondaryContainer: Color = Color(0xFF4A4458),
    onSecondaryContainer: Color = Color(0xFFE8DEF8),
    tertiary: Color = Color(0xFFEFB8C8), onTertiary: Color = Color(0xFF492532), tertiaryContainer: Color = Color(0xFF633B48),
    onTertiaryContainer: Color = Color(0xFFFFD8E4),
    background: Color = Color(0xFF141218), onBackground: Color = Color(0xFFE6E0E9), surface: Color = Color(0xFF141218),
    onSurface: Color = Color(0xFFE6E0E9), surfaceVariant: Color = Color(0xFF49454F), onSurfaceVariant: Color = Color(0xFFCAC4D0),
    surfaceTint: Color = primary, inverseSurface: Color = Color(0xFFE6E0E9), inverseOnSurface: Color = Color(0xFF322F35),
    error: Color = Color(0xFFF2B8B5), onError: Color = Color(0xFF601410), errorContainer: Color = Color(0xFF8C1D18),
    onErrorContainer: Color = Color(0xFFF9DEDC), outline: Color = Color(0xFF938F99), outlineVariant: Color = Color(0xFF49454F),
    scrim: Color = Color(0xFF000000), surfaceBright: Color = Color(0xFF3B383E), surfaceContainer: Color = Color(0xFF211F26),
    surfaceContainerHigh: Color = Color(0xFF2B2930), surfaceContainerHighest: Color = Color(0xFF36343B),
    surfaceContainerLow: Color = Color(0xFF1D1B20), surfaceContainerLowest: Color = Color(0xFF0F0D13), surfaceDim: Color = Color(0xFF141218),
) = ColorScheme(
    primary, onPrimary, primaryContainer, onPrimaryContainer, inversePrimary, secondary, onSecondary, secondaryContainer,
    onSecondaryContainer, tertiary, onTertiary, tertiaryContainer, onTertiaryContainer, background, onBackground, surface, onSurface,
    surfaceVariant, onSurfaceVariant, surfaceTint, inverseSurface, inverseOnSurface, error, onError, errorContainer, onErrorContainer,
    outline, outlineVariant, scrim, surfaceBright, surfaceContainer, surfaceContainerHigh, surfaceContainerHighest, surfaceContainerLow,
    surfaceContainerLowest, surfaceDim,
)

/** The `on*` color for a container color of this scheme, else the current content color. */
fun ColorScheme.contentColorFor(backgroundColor: Color): Color = when (backgroundColor) {
    primary -> onPrimary
    secondary -> onSecondary
    tertiary -> onTertiary
    background -> onBackground
    error -> onError
    primaryContainer -> onPrimaryContainer
    secondaryContainer -> onSecondaryContainer
    tertiaryContainer -> onTertiaryContainer
    errorContainer -> onErrorContainer
    inverseSurface -> inverseOnSurface
    surface, surfaceVariant, surfaceBright, surfaceContainer, surfaceContainerHigh, surfaceContainerHighest,
    surfaceContainerLow, surfaceContainerLowest, surfaceDim -> if (backgroundColor == surfaceVariant) onSurfaceVariant else onSurface
    else -> Color.Unspecified
}

@Composable
@ReadOnlyComposable
fun contentColorFor(backgroundColor: Color): Color =
    MaterialTheme.colorScheme.contentColorFor(backgroundColor).takeIf { it.isSpecified } ?: LocalContentColor.current

private fun style(size: Int, line: Int, tracking: Double, weight: FontWeight = FontWeight.Normal) =
    TextStyle(fontSize = size.sp, lineHeight = line.sp, letterSpacing = tracking.sp, fontWeight = weight)

/** Material 3 type scale (`TypographyTokens` as of material3 1.4.0, Roboto). */
@Immutable
data class Typography(
    val displayLarge: TextStyle = style(57, 64, -0.2),
    val displayMedium: TextStyle = style(45, 52, 0.0),
    val displaySmall: TextStyle = style(36, 44, 0.0),
    val headlineLarge: TextStyle = style(32, 40, 0.0),
    val headlineMedium: TextStyle = style(28, 36, 0.0),
    val headlineSmall: TextStyle = style(24, 32, 0.0),
    val titleLarge: TextStyle = style(22, 28, 0.0),
    val titleMedium: TextStyle = style(16, 24, 0.2, FontWeight.Medium),
    val titleSmall: TextStyle = style(14, 20, 0.1, FontWeight.Medium),
    val bodyLarge: TextStyle = style(16, 24, 0.5),
    val bodyMedium: TextStyle = style(14, 20, 0.2),
    val bodySmall: TextStyle = style(12, 16, 0.4),
    val labelLarge: TextStyle = style(14, 20, 0.1, FontWeight.Medium),
    val labelMedium: TextStyle = style(12, 16, 0.5, FontWeight.Medium),
    val labelSmall: TextStyle = style(11, 16, 0.5, FontWeight.Medium),
)

@Immutable
data class Shapes(
    val extraSmall: CornerBasedShape = RoundedCornerShape(4.dp),
    val small: CornerBasedShape = RoundedCornerShape(8.dp),
    val medium: CornerBasedShape = RoundedCornerShape(12.dp),
    val large: CornerBasedShape = RoundedCornerShape(16.dp),
    val extraLarge: CornerBasedShape = RoundedCornerShape(28.dp),
)

internal val LocalColorScheme = staticCompositionLocalOf { lightColorScheme() }
internal val LocalTypography = staticCompositionLocalOf { Typography() }
internal val LocalShapes = staticCompositionLocalOf { Shapes() }
val LocalContentColor = compositionLocalOf { Color.Black }
val LocalTextStyle = compositionLocalOf { TextStyle.Default }
val LocalMinimumInteractiveComponentSize = staticCompositionLocalOf { 48.dp }

object MaterialTheme {
    val colorScheme: ColorScheme @Composable @ReadOnlyComposable get() = LocalColorScheme.current
    val typography: Typography @Composable @ReadOnlyComposable get() = LocalTypography.current
    val shapes: Shapes @Composable @ReadOnlyComposable get() = LocalShapes.current
}

@Composable
fun MaterialTheme(
    colorScheme: ColorScheme = MaterialTheme.colorScheme,
    shapes: Shapes = MaterialTheme.shapes,
    typography: Typography = MaterialTheme.typography,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(
        LocalColorScheme provides colorScheme,
        LocalShapes provides shapes,
        LocalTypography provides typography,
        LocalTextSelectionColors provides TextSelectionColors(colorScheme.primary, colorScheme.primary.copy(alpha = 0.4f)),
    ) {
        ProvideTextStyle(typography.bodyLarge, content)
    }
}

@Composable
fun ProvideTextStyle(value: TextStyle, content: @Composable () -> Unit) {
    val merged = LocalTextStyle.current.merge(value)
    CompositionLocalProvider(LocalTextStyle provides merged, content = content)
}

/** Dynamic color needs a wallpaper; OneSim returns the baseline scheme. */
// ponytail: dynamic color is the baseline purple scheme, not wallpaper-derived.
fun dynamicLightColorScheme(context: Any?): ColorScheme = lightColorScheme()
fun dynamicDarkColorScheme(context: Any?): ColorScheme = darkColorScheme()
