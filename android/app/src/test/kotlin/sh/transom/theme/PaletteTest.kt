package sh.transom.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The web UI's stylesheet is the source of transom's colors, and the app's palette must match it
 * exactly, in light and in dark.
 */
class PaletteTest {
    @Test
    fun lightMatchesTheStylesheet() {
        assertEquals(stylesheetLight(), LightPalette.asCustomProperties())
    }

    @Test
    fun darkMatchesTheStylesheet() {
        assertEquals(stylesheetDark(), DarkPalette.asCustomProperties())
    }
}

private val stylesheet: String by lazy { File(System.getProperty("transom.css")!!).readText() }

private const val DARK_QUERY = "@media (prefers-color-scheme: dark)"

/** The custom properties of the first `:root` rule, which applies in light and dark. */
private fun stylesheetLight(): Map<String, String> =
    rootProperties(stylesheet.substringBefore(DARK_QUERY))

/** The custom properties of the `:root` rule inside the dark color scheme's media query. */
private fun stylesheetDark(): Map<String, String> =
    rootProperties(stylesheet.substringAfter(DARK_QUERY))

private val rootRule = Regex(""":root\s*\{([^}]*)}""")
private val colorProperty = Regex("""(--[a-z-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;""")

private fun rootProperties(css: String): Map<String, String> {
    val body = rootRule.find(css)!!.groupValues[1]
    return colorProperty.findAll(body).associate {
        it.groupValues[1] to it.groupValues[2].lowercase()
    }
}

private fun Palette.asCustomProperties(): Map<String, String> =
    mapOf(
        "--paper" to paper.hex(),
        "--ink" to ink.hex(),
        "--muted" to muted.hex(),
        "--rule" to rule.hex(),
        "--accent" to accent.hex(),
        "--accent-ink" to accentInk.hex(),
        "--soft" to soft.hex(),
        "--fire" to fire.hex(),
        "--warn" to warn.hex(),
    )

private fun Color.hex(): String = "#%06x".format(toArgb() and 0xffffff)
