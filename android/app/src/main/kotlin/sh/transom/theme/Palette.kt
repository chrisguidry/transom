package sh.transom.theme

import androidx.compose.ui.graphics.Color

/**
 * transom's colors, named for the custom properties of the web UI's `transom.css`. PaletteTest
 * fails when these differ from the stylesheet, so a change to one is a change to both.
 */
data class Palette(
    val paper: Color,
    val ink: Color,
    val muted: Color,
    val rule: Color,
    val accent: Color,
    val accentInk: Color,
    val soft: Color,
    /** The urgency dot of an urgent or high message. */
    val fire: Color,
    /** The urgency dot of a normal message. */
    val warn: Color,
)

val LightPalette =
    Palette(
        paper = Color(0xFFFBFBF9),
        ink = Color(0xFF23262B),
        muted = Color(0xFF6C6F75),
        rule = Color(0xFFE4E3DF),
        accent = Color(0xFF3D5A73),
        accentInk = Color(0xFFFFFFFF),
        soft = Color(0xFFF1F0EC),
        fire = Color(0xFFC2412D),
        warn = Color(0xFFB7791F),
    )

val DarkPalette =
    Palette(
        paper = Color(0xFF1B1C1E),
        ink = Color(0xFFE6E5E2),
        muted = Color(0xFF9A9B9F),
        rule = Color(0xFF33353A),
        accent = Color(0xFF8FB0CC),
        accentInk = Color(0xFF14171A),
        soft = Color(0xFF25272B),
        fire = Color(0xFFF2846F),
        warn = Color(0xFFE3B25A),
    )
