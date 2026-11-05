package sh.transom.inbox

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import sh.transom.MainActivity
import sh.transom.theme.DarkPalette
import sh.transom.theme.LightPalette

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class EmptyInboxTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun showsTheWordmark() {
        compose.onNodeWithText("transom").assertIsDisplayed()
    }

    @Test
    fun headsThePage() {
        compose.onNodeWithText("Inbox").assert(isHeading())
    }

    @Test
    fun saysThereIsNothingHereYet() {
        compose.onNodeWithText("Nothing here yet.").assertIsDisplayed()
    }

    @Test
    fun paintsLightPaper() {
        assertEquals(LightPalette.paper, cornerColor())
    }

    @Test
    @Config(qualifiers = "night")
    fun paintsDarkPaperWhenTheSystemIsDark() {
        assertEquals(DarkPalette.paper, cornerColor())
    }

    /** The color of the screen's bottom-right corner, where nothing but the page is drawn. */
    private fun cornerColor(): Color {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        return pixels[pixels.width - 1, pixels.height - 1]
    }
}
