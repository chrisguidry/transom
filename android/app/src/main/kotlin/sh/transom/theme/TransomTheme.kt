package sh.transom.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable

/**
 * transom's palette in light or dark, as the system is set. The app never uses Android's dynamic
 * color, so it looks the same as the web UI on every phone. The system font is Material's default,
 * so the typography is left alone.
 */
@Composable
fun TransomTheme(content: @Composable () -> Unit) {
    val colorScheme =
        if (isSystemInDarkTheme()) {
            darkColorScheme().withPalette(DarkPalette)
        } else {
            lightColorScheme().withPalette(LightPalette)
        }
    MaterialTheme(colorScheme = colorScheme, content = content)
}

/**
 * Material's color roles drawn from the palette, so no Material component shows a color the web UI
 * doesn't have: the accent for every emphasis, soft for every raised surface, and rule for every
 * outline.
 */
private fun ColorScheme.withPalette(palette: Palette): ColorScheme =
    copy(
        primary = palette.accent,
        onPrimary = palette.accentInk,
        primaryContainer = palette.accent,
        onPrimaryContainer = palette.accentInk,
        secondary = palette.accent,
        onSecondary = palette.accentInk,
        secondaryContainer = palette.soft,
        onSecondaryContainer = palette.ink,
        tertiary = palette.accent,
        onTertiary = palette.accentInk,
        tertiaryContainer = palette.soft,
        onTertiaryContainer = palette.ink,
        background = palette.paper,
        onBackground = palette.ink,
        surface = palette.paper,
        onSurface = palette.ink,
        surfaceVariant = palette.soft,
        onSurfaceVariant = palette.muted,
        surfaceTint = palette.accent,
        surfaceBright = palette.paper,
        surfaceDim = palette.soft,
        surfaceContainerLowest = palette.paper,
        surfaceContainerLow = palette.soft,
        surfaceContainer = palette.soft,
        surfaceContainerHigh = palette.soft,
        surfaceContainerHighest = palette.soft,
        inverseSurface = palette.ink,
        inverseOnSurface = palette.paper,
        inversePrimary = palette.paper,
        outline = palette.rule,
        outlineVariant = palette.rule,
    )
